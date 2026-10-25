//! Surface & rendering: RGBA pixel buffer, graphics state, path rasterisation.

use std::sync::Arc;

use super::clip::ClipMask;
use super::fonts::FontMetrics;

#[derive(Debug, Clone, Copy)]
pub(super) struct Color {
    r: u8,
    g: u8,
    b: u8,
}

impl Color {
    const BLACK: Color = Color { r: 0, g: 0, b: 0 };

    pub(super) fn from_gray(v: f32) -> Self {
        let c = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color { r: c, g: c, b: c }
    }
    pub(super) fn from_rgb(r: f32, g: f32, b: f32) -> Self {
        Color {
            r: (r.clamp(0.0, 1.0) * 255.0).round() as u8,
            g: (g.clamp(0.0, 1.0) * 255.0).round() as u8,
            b: (b.clamp(0.0, 1.0) * 255.0).round() as u8,
        }
    }
}

/// The graphics state that `q` / `Q` save and restore.
#[derive(Debug, Clone)]
pub(super) struct State {
    pub(super) fill: Color,
    pub(super) stroke: Color,
    pub(super) line_width: f32,
    /// Constant alpha for fill operations (`/ca` in an `/ExtGState`).
    pub(super) fill_alpha: f32,
    /// Constant alpha for stroke operations (`/CA` in an `/ExtGState`).
    pub(super) stroke_alpha: f32,
    /// Text rendering mode (`Tr`): 0 fill, 1 stroke, 2 fill+stroke, 3 invisible.
    pub(super) text_render_mode: u8,
    /// Active clipping region, if any.
    pub(super) clip: Option<Arc<ClipMask>>,
}

impl Default for State {
    fn default() -> Self {
        State {
            fill: Color::BLACK,
            stroke: Color::BLACK,
            line_width: 1.0,
            fill_alpha: 1.0,
            stroke_alpha: 1.0,
            text_render_mode: 0,
            clip: None,
        }
    }
}

pub(super) struct Surface {
    pub(super) pixels: Vec<u8>,
    pub(super) width: u32,
    pub(super) height: u32,
    /// CTM: [a, b, c, d, e, f] in PDF coords (origin bottom-left).
    pub(super) transform: [f32; 6],
    pub(super) state_stack: Vec<State>,
    pub(super) state: State,
    /// Path under construction, in PDF user space.
    pub(super) path: Vec<PathSegment>,
    pub(super) subpath_start: (f32, f32),
    pub(super) current_point: (f32, f32),
    /// True while inside BT..ET (text object mode).
    pub(super) in_text: bool,
    /// Text matrix [a, b, c, d, e, f].
    pub(super) text_matrix: [f32; 6],
    /// Text line matrix (for T*)
    pub(super) text_line_matrix: [f32; 6],
    /// Current font size in points.
    pub(super) font_size: f32,
    /// Current font metrics (by font name, e.g. "F1").
    pub(super) font_metrics: Option<FontMetrics>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum PathSegment {
    Move(f32, f32),
    Line(f32, f32),
    Curve(f32, f32, f32, f32, f32, f32),
    Close,
}

impl Surface {
    pub(super) fn new(width: u32, height: u32) -> Self {
        let pixels = vec![255u8; (width as usize) * (height as usize) * 4];
        Surface {
            pixels,
            width,
            height,
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            state_stack: Vec::new(),
            state: State::default(),
            path: Vec::new(),
            subpath_start: (0.0, 0.0),
            current_point: (0.0, 0.0),
            in_text: false,
            text_matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            text_line_matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            font_size: 12.0,
            font_metrics: None,
        }
    }

    pub(super) fn set_pixel(&mut self, x: u32, y: u32, color: Color, alpha: f32) {
        if x >= self.width || y >= self.height {
            return;
        }
        if let Some(clip) = &self.state.clip
            && !clip.contains(x as i32, y as i32)
        {
            return;
        }
        let i = ((y * self.width + x) as usize) * 4;
        let a = alpha.clamp(0.0, 1.0);
        if a >= 1.0 {
            self.pixels[i] = color.r;
            self.pixels[i + 1] = color.g;
            self.pixels[i + 2] = color.b;
            self.pixels[i + 3] = 255;
        } else {
            let inv = 1.0 - a;
            self.pixels[i] = (color.r as f32 * a + self.pixels[i] as f32 * inv) as u8;
            self.pixels[i + 1] = (color.g as f32 * a + self.pixels[i + 1] as f32 * inv) as u8;
            self.pixels[i + 2] = (color.b as f32 * a + self.pixels[i + 2] as f32 * inv) as u8;
            self.pixels[i + 3] = 255;
        }
    }

    /// Transform PDF (x, y) by the current CTM, returning screen coords (px, py).
    pub(super) fn transform_pt(&self, x: f32, y: f32) -> (f32, f32) {
        let m = self.transform;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }

    pub(super) fn stroke_path(&mut self) {
        if self.path.is_empty() {
            return;
        }
        // Flatten to line segments, then stroke.
        let mut segments: Vec<(f32, f32, f32, f32)> = Vec::new();
        let mut start = self.subpath_start;
        let mut cur = self.current_point;
        for seg in &self.path {
            match seg {
                PathSegment::Move(x, y) => {
                    start = (*x, *y);
                    cur = (*x, *y);
                }
                PathSegment::Line(x, y) => {
                    segments.push((cur.0, cur.1, *x, *y));
                    cur = (*x, *y);
                }
                PathSegment::Curve(x1, y1, x2, y2, x3, y3) => {
                    flatten_cubic_into_segments(
                        &mut segments,
                        cur,
                        (*x1, *y1),
                        (*x2, *y2),
                        (*x3, *y3),
                    );
                    cur = (*x3, *y3);
                }
                PathSegment::Close => {
                    segments.push((cur.0, cur.1, start.0, start.1));
                    cur = start;
                }
            }
        }
        let lw = self.state.line_width.max(0.5);
        let device: Vec<(f32, f32, f32, f32)> = segments
            .iter()
            .map(|(a, b, c, d)| {
                let (x1, y1) = self.transform_pt(*a, *b);
                let (x2, y2) = self.transform_pt(*c, *d);
                (x1, y1, x2, y2)
            })
            .collect();
        let radius = self.stroke_radius(lw);
        self.stroke_device_segments(&device, radius);
    }

    /// The device-space radius of a stroke of the given user-space width.
    fn stroke_radius(&self, line_width: f32) -> f32 {
        (line_width * 0.5 * (self.transform[0].abs() + self.transform[3].abs()) * 0.5).max(0.5)
    }

    pub(super) fn fill_path(&mut self) {
        if self.path.is_empty() {
            return;
        }
        for poly in self.flatten_path() {
            self.fill_polygon(&poly);
        }
    }

    /// Flatten the current path into user-space subpaths.
    pub(super) fn flatten_path(&self) -> Vec<Vec<(f32, f32)>> {
        let mut polys: Vec<Vec<(f32, f32)>> = Vec::new();
        let mut current: Vec<(f32, f32)> = Vec::new();
        let mut start = self.subpath_start;
        let mut cur = self.current_point;
        for seg in &self.path {
            match seg {
                PathSegment::Move(x, y) => {
                    if !current.is_empty() {
                        polys.push(std::mem::take(&mut current));
                    }
                    start = (*x, *y);
                    current.push((*x, *y));
                    cur = (*x, *y);
                }
                PathSegment::Line(x, y) => {
                    current.push((*x, *y));
                    cur = (*x, *y);
                }
                PathSegment::Curve(x1, y1, x2, y2, x3, y3) => {
                    flatten_cubic_into(&mut current, cur, (*x1, *y1), (*x2, *y2), (*x3, *y3));
                    cur = (*x3, *y3);
                }
                PathSegment::Close => {
                    if !current.is_empty() {
                        polys.push(std::mem::take(&mut current));
                    }
                    cur = start;
                }
            }
        }
        if !current.is_empty() {
            polys.push(current);
        }
        polys
    }

    /// The current path as device-space subpaths, for clipping.
    pub(super) fn path_device_polygons(&self) -> Vec<Vec<(f32, f32)>> {
        self.flatten_path()
            .into_iter()
            .map(|poly| {
                poly.into_iter()
                    .map(|(x, y)| self.transform_pt(x, y))
                    .collect()
            })
            .collect()
    }

    /// Install a clipping region, intersecting it with the active one.
    pub(super) fn set_clip(&mut self, mask: ClipMask) {
        let combined = match &self.state.clip {
            Some(existing) => existing.intersect(&mask),
            None => mask,
        };
        self.state.clip = Some(Arc::new(combined));
    }

    /// Stroke a polyline given in device coordinates.
    ///
    /// Used for text rendering mode 1 (stroke) and 2 (fill + stroke), where the
    /// glyph outline is drawn as an outline rather than a solid shape.
    pub(super) fn stroke_device_polyline(&mut self, points: &[(f32, f32)], close: bool) {
        if points.len() < 2 {
            return;
        }
        let mut segments: Vec<(f32, f32, f32, f32)> = Vec::new();
        for i in 0..points.len() - 1 {
            segments.push((points[i].0, points[i].1, points[i + 1].0, points[i + 1].1));
        }
        if close {
            let last = points[points.len() - 1];
            segments.push((last.0, last.1, points[0].0, points[0].1));
        }
        let lw = self.state.line_width.max(0.5);
        let radius = self.stroke_radius(lw);
        self.stroke_device_segments(&segments, radius);
    }

    /// Stroke device-space segments with a round brush.
    ///
    /// Coverage is accumulated per pixel (taking the maximum over all
    /// segments) and blended exactly once, so a partially transparent stroke
    /// is not compounded by overlapping segments.
    fn stroke_device_segments(&mut self, segments: &[(f32, f32, f32, f32)], radius: f32) {
        if segments.is_empty() {
            return;
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for (x1, y1, x2, y2) in segments {
            for (x, y) in [(x1, y1), (x2, y2)] {
                if !x.is_finite() || !y.is_finite() {
                    continue;
                }
                min_x = min_x.min(*x);
                min_y = min_y.min(*y);
                max_x = max_x.max(*x);
                max_y = max_y.max(*y);
            }
        }
        if min_x > max_x || min_y > max_y {
            return;
        }
        let x0 = ((min_x - radius - 1.0).floor() as i32).max(0);
        let y0 = ((min_y - radius - 1.0).floor() as i32).max(0);
        let x1 = ((max_x + radius + 1.0).ceil() as i32).min(self.width as i32 - 1);
        let y1 = ((max_y + radius + 1.0).ceil() as i32).min(self.height as i32 - 1);
        if x1 < x0 || y1 < y0 {
            return;
        }
        let w = (x1 - x0 + 1) as usize;
        let h = (y1 - y0 + 1) as usize;
        // Coverage is stored as 0..=255 to keep the scratch buffer small even
        // for page-sized strokes.
        let mut coverage = vec![0u8; w * h];

        for (sx1, sy1, sx2, sy2) in segments {
            let (sx1, sy1, sx2, sy2) = (*sx1, *sy1, *sx2, *sy2);
            if !sx1.is_finite() || !sy1.is_finite() || !sx2.is_finite() || !sy2.is_finite() {
                continue;
            }
            let bx0 = ((sx1.min(sx2) - radius - 1.0).floor() as i32).max(x0);
            let bx1 = ((sx1.max(sx2) + radius + 1.0).ceil() as i32).min(x1);
            let by0 = ((sy1.min(sy2) - radius - 1.0).floor() as i32).max(y0);
            let by1 = ((sy1.max(sy2) + radius + 1.0).ceil() as i32).min(y1);
            for py in by0..=by1 {
                let fy = py as f32 + 0.5;
                for px in bx0..=bx1 {
                    let fx = px as f32 + 0.5;
                    let distance = point_segment_distance(fx, fy, sx1, sy1, sx2, sy2);
                    let value = ((radius + 0.5 - distance).clamp(0.0, 1.0) * 255.0).round() as u8;
                    let index = (py - y0) as usize * w + (px - x0) as usize;
                    if value > coverage[index] {
                        coverage[index] = value;
                    }
                }
            }
        }

        let color = self.state.stroke;
        let alpha = self.state.stroke_alpha;
        for py in y0..=y1 {
            for px in x0..=x1 {
                let index = (py - y0) as usize * w + (px - x0) as usize;
                let value = coverage[index];
                if value > 0 {
                    self.set_pixel(px as u32, py as u32, color, (value as f32 / 255.0) * alpha);
                }
            }
        }
    }

    fn fill_polygon(&mut self, poly: &[(f32, f32)]) {
        if poly.len() < 3 {
            return;
        }
        // Transform to screen coords.
        let screen: Vec<(f32, f32)> = poly.iter().map(|&(x, y)| self.transform_pt(x, y)).collect();
        let color = self.state.fill;
        let alpha = self.state.fill_alpha;
        let mut min_y = screen[0].1;
        let mut max_y = screen[0].1;
        for p in &screen {
            if p.1 < min_y {
                min_y = p.1;
            }
            if p.1 > max_y {
                max_y = p.1;
            }
        }
        let y_start = (min_y.floor() as i32).max(0);
        let y_end = (max_y.ceil() as i32).min(self.height as i32 - 1);
        for y in y_start..=y_end {
            let yf = y as f32 + 0.5;
            let mut xs: Vec<f32> = Vec::new();
            let n = screen.len();
            for i in 0..n {
                let (x1, y1) = screen[i];
                let (x2, y2) = screen[(i + 1) % n];
                if (y1 <= yf && y2 > yf) || (y2 <= yf && y1 > yf) {
                    let t = (yf - y1) / (y2 - y1);
                    xs.push(x1 + (x2 - x1) * t);
                }
            }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mut i = 0;
            while i + 1 < xs.len() {
                let xa = xs[i];
                let xb = xs[i + 1];
                let x_start = (xa.ceil() as i32).max(0);
                let x_end = (xb.floor() as i32).min(self.width as i32 - 1);
                if x_end >= x_start {
                    for x in x_start..=x_end {
                        self.set_pixel(x as u32, y as u32, color, alpha);
                    }
                }
                i += 2;
            }
        }
    }
}

/// Distance from a point to a line segment.
fn point_segment_distance(px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length_sq = dx * dx + dy * dy;
    if length_sq <= f32::EPSILON {
        return (px - x1).hypot(py - y1);
    }
    let t = (((px - x1) * dx + (py - y1) * dy) / length_sq).clamp(0.0, 1.0);
    (px - (x1 + t * dx)).hypot(py - (y1 + t * dy))
}

fn flatten_cubic_into(
    poly: &mut Vec<(f32, f32)>,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
) {
    flatten_cubic_points(poly, p0, p1, p2, p3);
}

// Tessellate cubic Bezier into segments/points via midpoint subdivision.
fn flatten_cubic_into_segments(
    out: &mut Vec<(f32, f32, f32, f32)>,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
) {
    let mut stack = vec![(p0, p1, p2, p3)];
    while let Some((a, b, c, d)) = stack.pop() {
        if cubic_is_flat(a, b, c, d) {
            out.push((a.0, a.1, d.0, d.1));
        } else {
            let m01 = midpoint(a, b);
            let m12 = midpoint(b, c);
            let m23 = midpoint(c, d);
            let m012 = midpoint(m01, m12);
            let m123 = midpoint(m12, m23);
            let m0123 = midpoint(m012, m123);
            // LIFO stack: push the second half first so the first half is
            // flattened (and emitted) in path order.
            stack.push((m0123, m123, m23, d));
            stack.push((a, m01, m012, m0123));
        }
    }
}

/// Flatness test for a cubic Bézier, after the "Anti-Grain Geometry" method.
///
/// The perpendicular distance of both control points from the chord is
/// compared against a tolerance. A simpler "control polygon length minus
/// chord length" test is wrong: it goes negative for a curve that bulges away
/// from its chord, which silently collapses the curve to a straight line.
fn cubic_is_flat(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> bool {
    let dx = d.0 - a.0;
    let dy = d.1 - a.1;
    let chord_sq = dx * dx + dy * dy;
    if chord_sq < 0.25 {
        return true;
    }
    let d2 = ((b.0 - a.0) * dy - (b.1 - a.1) * dx).abs();
    let d3 = ((c.0 - a.0) * dy - (c.1 - a.1) * dx).abs();
    (d2 + d3) <= flatness_tolerance(chord_sq)
}

/// The deviation allowed between a curve and its chord.
fn flatness_tolerance(chord_sq: f32) -> f32 {
    // 0.5 % of the chord length, with an absolute floor so that tiny curves
    // still terminate.
    (0.005 * chord_sq).sqrt().max(0.25)
}

/// Flatness test for a quadratic Bézier.
fn quad_is_flat(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let dx = c.0 - a.0;
    let dy = c.1 - a.1;
    let chord_sq = dx * dx + dy * dy;
    if chord_sq < 0.25 {
        return true;
    }
    let d = ((b.0 - a.0) * dy - (b.1 - a.1) * dx).abs();
    d <= flatness_tolerance(chord_sq)
}

fn flatten_cubic_points(
    poly: &mut Vec<(f32, f32)>,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
) {
    let mut stack = vec![(p0, p1, p2, p3)];
    let mut started = false;
    while let Some((a, b, c, d)) = stack.pop() {
        if cubic_is_flat(a, b, c, d) {
            if !started {
                poly.push(a);
                started = true;
            }
            poly.push(d);
        } else {
            let m01 = midpoint(a, b);
            let m12 = midpoint(b, c);
            let m23 = midpoint(c, d);
            let m012 = midpoint(m01, m12);
            let m123 = midpoint(m12, m23);
            let m0123 = midpoint(m012, m123);
            // The stack is LIFO, so the second half is pushed first: the
            // first half is then popped, and therefore flattened, first.
            // Pushing them the other way round emits the points out of order
            // and scrambles the curve.
            stack.push((m0123, m123, m23, d));
            stack.push((a, m01, m012, m0123));
        }
    }
}

fn midpoint(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
}

/// Fill device-space polygons with the non-zero winding rule.
///
/// All contours of a shape are filled together, so the counter of a letter
/// ("o", "e", "p") stays hollow instead of being filled solid — TrueType
/// outlines express counters as a second contour wound in the opposite
/// direction.
pub(super) fn fill_polygons_raw(surface: &mut Surface, polys: &[Vec<(f32, f32)>], color: Color) {
    let alpha = surface.state.fill_alpha;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for poly in polys {
        for p in poly {
            if !p.1.is_finite() {
                continue;
            }
            min_y = min_y.min(p.1);
            max_y = max_y.max(p.1);
        }
    }
    if min_y > max_y {
        return;
    }
    let y_start = (min_y.floor() as i32).max(0);
    let y_end = (max_y.ceil() as i32).min(surface.height as i32 - 1);
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for y in y_start..=y_end {
        let yf = y as f32 + 0.5;
        crossings.clear();
        for poly in polys {
            let n = poly.len();
            if n < 2 {
                continue;
            }
            for i in 0..n {
                let (x1, y1) = poly[i];
                let (x2, y2) = poly[(i + 1) % n];
                if y1 == y2 || !y1.is_finite() || !y2.is_finite() {
                    continue;
                }
                // Half-open vertical interval: shared vertices are counted once.
                let (lo, hi, dir) = if y1 < y2 { (y1, y2, 1) } else { (y2, y1, -1) };
                if yf < lo || yf >= hi {
                    continue;
                }
                let t = (yf - y1) / (y2 - y1);
                crossings.push((x1 + (x2 - x1) * t, dir));
            }
        }
        if crossings.len() < 2 {
            continue;
        }
        crossings.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut winding = 0;
        for i in 0..crossings.len() - 1 {
            winding += crossings[i].1;
            if winding == 0 {
                continue;
            }
            let xa = crossings[i].0;
            let xb = crossings[i + 1].0;
            let x_start = (xa.ceil() as i32).max(0);
            let x_end = (xb.floor() as i32).min(surface.width as i32 - 1);
            if x_end >= x_start {
                for x in x_start..=x_end {
                    surface.set_pixel(x as u32, y as u32, color, alpha);
                }
            }
        }
    }
}

/// Builder that collects glyph outline contours from `ttf_parser`.
pub(super) struct GlyphOutlineBuilder {
    pub(super) contours: Vec<Vec<(f32, f32)>>,
    current: Vec<(f32, f32)>,
    start: (f32, f32),
    last: (f32, f32),
}

impl GlyphOutlineBuilder {
    pub(super) fn new() -> Self {
        GlyphOutlineBuilder {
            contours: Vec::new(),
            current: Vec::new(),
            start: (0.0, 0.0),
            last: (0.0, 0.0),
        }
    }
}

impl ttf_parser::OutlineBuilder for GlyphOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        if !self.current.is_empty() {
            self.contours.push(std::mem::take(&mut self.current));
        }
        self.start = (x, y);
        self.last = (x, y);
        self.current.push((x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.current.push((x, y));
        self.last = (x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        // Flatten quadratic Bézier via midpoint subdivision.
        flatten_quad(&mut self.current, self.last, (x1, y1), (x, y));
        self.last = (x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        flatten_cubic_points(&mut self.current, self.last, (x1, y1), (x2, y2), (x, y));
        self.last = (x, y);
    }

    fn close(&mut self) {
        if !self.current.is_empty() {
            self.contours.push(std::mem::take(&mut self.current));
        }
        self.last = self.start;
    }
}

/// Flatten a quadratic Bézier into line segments using midpoint subdivision.
fn flatten_quad(out: &mut Vec<(f32, f32)>, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32)) {
    let mut stack = vec![(p0, p1, p2)];
    while let Some((a, b, c)) = stack.pop() {
        if quad_is_flat(a, b, c) {
            out.push(c);
        } else {
            let m01 = midpoint(a, b);
            let m12 = midpoint(b, c);
            let m012 = midpoint(m01, m12);
            // LIFO stack: push the second half first so the points are
            // emitted in path order.
            stack.push((m012, m12, c));
            stack.push((a, m01, m012));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cubic_outlines_are_flattened_in_path_order() {
        // A cubic Bézier flattened into points must keep its order, otherwise
        // the outline self-intersects and the glyph renders as a blob.
        let mut flattened = Vec::new();
        flatten_cubic_points(
            &mut flattened,
            (0.0, 0.0),
            (50.0, 100.0),
            (100.0, 0.0),
            (100.0, 0.0),
        );
        assert!(flattened.len() > 2);
        // Every step must be short: a scrambled flattening produces long jumps.
        for pair in flattened.windows(2) {
            let step = (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1);
            assert!(
                step < 10.0,
                "flattened points are out of order: step {step}"
            );
        }
        // The curve starts at the first control point and ends at the last.
        assert!(flattened[0].0.abs() < 1.0 && flattened[0].1.abs() < 1.0);
        let last = flattened[flattened.len() - 1];
        assert!((last.0 - 100.0).abs() < 1.0 && last.1.abs() < 1.0);
    }

    #[test]
    fn cubic_segments_are_flattened_in_path_order() {
        let mut segments = Vec::new();
        flatten_cubic_into_segments(
            &mut segments,
            (0.0, 0.0),
            (50.0, 100.0),
            (100.0, 0.0),
            (100.0, 0.0),
        );
        assert!(segments.len() > 2);
        for (a, b, c, d) in &segments {
            let step = (c - a).hypot(d - b);
            assert!(
                step < 10.0,
                "flattened segments are out of order: step {step}"
            );
        }
        assert!((segments[0].0 - 0.0).abs() < 1.0 && (segments[0].1 - 0.0).abs() < 1.0);
        let last = segments[segments.len() - 1];
        assert!((last.2 - 100.0).abs() < 1.0 && last.3.abs() < 1.0);
    }
}
