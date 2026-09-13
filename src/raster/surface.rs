//! Surface & rendering: RGBA pixel buffer, graphics state, path rasterisation.

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

#[derive(Debug, Clone, Copy)]
pub(super) struct State {
    pub(super) fill: Color,
    pub(super) stroke: Color,
    pub(super) line_width: f32,
}

impl Default for State {
    fn default() -> Self {
        State {
            fill: Color::BLACK,
            stroke: Color::BLACK,
            line_width: 1.0,
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
        for (a, b, c, d) in segments {
            self.draw_line(a, b, c, d, lw);
        }
    }

    pub(super) fn fill_path(&mut self) {
        if self.path.is_empty() {
            return;
        }
        // Build a flattened polygon of the current subpath(s).
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
        for poly in polys {
            self.fill_polygon(&poly);
        }
    }

    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, lw: f32) {
        let (sx1, sy1) = self.transform_pt(x1, y1);
        let (sx2, sy2) = self.transform_pt(x2, y2);
        let color = self.state.stroke;
        let radius =
            (lw * 0.5 * (self.transform[0].abs() + self.transform[3].abs()) * 0.5).max(0.5);

        // Use Bresenham-style with a thin brush radius for anti-aliased strokes.
        let steps = ((sx2 - sx1).hypot(sy2 - sy1).ceil() as i32 + 2).max(2);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let px = sx1 + (sx2 - sx1) * t;
            let py = sy1 + (sy2 - sy1) * t;
            self.disc(px.round() as i32, py.round() as i32, radius, color);
        }
    }

    fn disc(&mut self, cx: i32, cy: i32, r: f32, color: Color) {
        let r2 = r.ceil() as i32 + 1;
        let r_sq = r * r;
        for dy in -r2..=r2 {
            for dx in -r2..=r2 {
                let dist_sq = (dx * dx + dy * dy) as f32;
                if dist_sq <= r_sq {
                    let alpha = 1.0 - (dist_sq.sqrt() / r).clamp(0.0, 1.0);
                    let x = (cx + dx) as u32;
                    let y = (cy + dy) as u32;
                    self.set_pixel(x, y, color, alpha);
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
                        self.set_pixel(x as u32, y as u32, color, 1.0);
                    }
                }
                i += 2;
            }
        }
    }
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
        let chord = (d.0 - a.0).hypot(d.1 - a.1);
        let poly1 = (b.0 - a.0).hypot(b.1 - a.1);
        let poly2 = (c.0 - d.0).hypot(c.1 - d.1);
        let total = poly1 + poly2;
        if chord < 0.5 || total - chord < 0.5 || (a == b && c == d) {
            out.push((a.0, a.1, d.0, d.1));
        } else {
            let m01 = midpoint(a, b);
            let m12 = midpoint(b, c);
            let m23 = midpoint(c, d);
            let m012 = midpoint(m01, m12);
            let m123 = midpoint(m12, m23);
            let m0123 = midpoint(m012, m123);
            stack.push((a, m01, m012, m0123));
            stack.push((m0123, m123, m23, d));
        }
    }
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
        let chord = (d.0 - a.0).hypot(d.1 - a.1);
        let poly1 = (b.0 - a.0).hypot(b.1 - a.1);
        let poly2 = (c.0 - d.0).hypot(c.1 - d.1);
        let total = poly1 + poly2;
        if chord < 0.5 || total - chord < 0.5 {
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
            stack.push((a, m01, m012, m0123));
            stack.push((m0123, m123, m23, d));
        }
    }
}

fn midpoint(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
}

/// Fill a polygon given in screen coordinates directly (no CTM applied).
pub(super) fn fill_polygon_raw(surface: &mut Surface, poly: &[(f32, f32)], color: Color) {
    if poly.len() < 3 {
        return;
    }
    let mut min_y = poly[0].1;
    let mut max_y = poly[0].1;
    for p in poly {
        if p.1 < min_y {
            min_y = p.1;
        }
        if p.1 > max_y {
            max_y = p.1;
        }
    }
    let y_start = (min_y.floor() as i32).max(0);
    let y_end = (max_y.ceil() as i32).min(surface.height as i32 - 1);
    for y in y_start..=y_end {
        let yf = y as f32 + 0.5;
        let mut xs: Vec<f32> = Vec::new();
        let n = poly.len();
        for i in 0..n {
            let (x1, y1) = poly[i];
            let (x2, y2) = poly[(i + 1) % n];
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
            let x_end = (xb.floor() as i32).min(surface.width as i32 - 1);
            if x_end >= x_start {
                for x in x_start..=x_end {
                    surface.set_pixel(x as u32, y as u32, color, 1.0);
                }
            }
            i += 2;
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
        let chord = (c.0 - a.0).hypot(c.1 - a.1);
        let poly = (b.0 - a.0).hypot(b.1 - a.1) + (c.0 - b.0).hypot(c.1 - b.1);
        if chord < 0.5 || poly - chord < 0.5 {
            out.push(c);
        } else {
            let m01 = midpoint(a, b);
            let m12 = midpoint(b, c);
            let m012 = midpoint(m01, m12);
            stack.push((a, m01, m012));
            stack.push((m012, m12, c));
        }
    }
}
