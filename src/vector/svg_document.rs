//! Full SVG document model and renderer: groups, transforms, basic shapes,
//! text, `<use>`/`<defs>`, and styling attributes.

use super::path::parse_svg_path;
use super::transform::parse_svg_transform;
use super::xml::parse_svg_xml;
use super::{PathOp, VectorCanvas, VectorShape};
use crate::error::Result;
use crate::pdf_generator::{Color, PageLayout};
use std::collections::HashMap;

// ----- Full SVG document support ------------------------------------------

/// Parse a full SVG document into a [`VectorCanvas`], honoring groups,
/// transforms, basic shapes, text, and styling attributes.
///
/// Supported elements:
/// - `<svg>` root with `viewBox` and `width`/`height`
/// - `<g>` with `transform="translate|rotate|scale|matrix(...)"`
/// - `<rect>`, `<circle>`, `<ellipse>`, `<line>`, `<polyline>`, `<polygon>`
/// - `<path d="...">` (delegates to [`parse_svg_path`])
/// - `<text>` with `x`, `y`, `font-size`, `fill`
///
/// Style attributes (`fill`, `stroke`, `stroke-width`, `opacity`) on elements
/// and inherited via parent `<g>` are honoured. The PDF coordinate system
/// (origin bottom-left, +Y up) is reconciled with SVG (origin top-left, +Y
/// down) by flipping the canvas vertically against the page height.
pub fn parse_svg_document(svg: &str, layout: PageLayout) -> Result<VectorCanvas> {
    let element_tree = parse_svg_xml(svg)?;
    let root_node = SvgNode::Element(element_tree);
    let mut ctx = RenderCtx {
        canvas: VectorCanvas::new(),
        transform_stack: vec![TransformStackEntry {
            // Flip Y so SVG top-left origin maps to PDF bottom-left origin.
            matrix: [1.0, 0.0, 0.0, -1.0, 0.0, layout.height],
        }],
        fill: SvgPaint::Color(Color::black()),
        stroke: SvgPaint::None,
        stroke_width: f32::NAN,
        defs: HashMap::new(),
    };
    if let SvgNode::Element(ref root_el) = root_node {
        collect_defs(root_el, &mut ctx);
        apply_svg_viewbox(root_el, &mut ctx, layout);
    }
    render_element(&root_node, &mut ctx);
    Ok(ctx.canvas)
}

/// Render a parsed SVG document to PDF bytes on a single page.
pub fn svg_document_to_pdf_bytes(svg: &str, layout: PageLayout) -> Result<Vec<u8>> {
    let canvas = parse_svg_document(svg, layout)?;
    canvas.to_pdf_bytes(layout)
}

/// Render a full SVG file (with groups, transforms, shapes, text) to a PDF.
pub fn svg_document_file_to_pdf(
    svg_file: &str,
    output_pdf: &str,
    layout: PageLayout,
) -> Result<()> {
    let svg = std::fs::read_to_string(svg_file)?;
    let bytes = svg_document_to_pdf_bytes(&svg, layout)?;
    std::fs::write(output_pdf, bytes)?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct TransformStackEntry {
    /// 2×3 affine matrix: [a, b, c, d, e, f] mapping (x,y) → (a*x+c*y+e, b*x+d*y+f).
    matrix: [f32; 6],
}

impl TransformStackEntry {
    fn identity() -> Self {
        TransformStackEntry {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }
    }

    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        let m = self.matrix;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }

    fn concat(&self, other: &[f32; 6]) -> TransformStackEntry {
        // Compose: result = self ∘ other (other applies first).
        let a = self.matrix;
        let b = *other;
        TransformStackEntry {
            matrix: [
                a[0] * b[0] + a[2] * b[1],
                a[1] * b[0] + a[3] * b[1],
                a[0] * b[2] + a[2] * b[3],
                a[1] * b[2] + a[3] * b[3],
                a[0] * b[4] + a[2] * b[5] + a[4],
                a[1] * b[4] + a[3] * b[5] + a[5],
            ],
        }
    }
}

#[derive(Debug, Clone)]
enum SvgPaint {
    Inherit,
    Color(Color),
    None,
}

impl SvgPaint {
    fn resolve(&self, fallback: Option<Color>) -> Option<Color> {
        match self {
            SvgPaint::Color(c) => Some(*c),
            SvgPaint::None => None,
            SvgPaint::Inherit => fallback,
        }
    }
}

struct RenderCtx {
    canvas: VectorCanvas,
    transform_stack: Vec<TransformStackEntry>,
    fill: SvgPaint,
    stroke: SvgPaint,
    stroke_width: f32,
    defs: HashMap<String, SvgElement>,
}

impl RenderCtx {
    fn current(&self) -> TransformStackEntry {
        *self
            .transform_stack
            .last()
            .unwrap_or(&TransformStackEntry::identity())
    }
    fn push_transform(&mut self, m: [f32; 6]) {
        let parent = self.current();
        self.transform_stack.push(parent.concat(&m));
    }
    fn pop_transform(&mut self) {
        if self.transform_stack.len() > 1 {
            self.transform_stack.pop();
        }
    }
    fn fill_color(&self) -> Option<Color> {
        self.fill.resolve(Some(Color::black()))
    }
    fn stroke_color(&self) -> Option<Color> {
        self.stroke.resolve(None)
    }
    fn line_width(&self) -> f32 {
        if self.stroke_width.is_nan() {
            1.0
        } else {
            self.stroke_width
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum SvgNode {
    Element(SvgElement),
    Text(String),
}

#[derive(Debug, Clone)]
pub(super) struct SvgElement {
    pub(super) name: String,
    pub(super) attrs: HashMap<String, String>,
    pub(super) children: Vec<SvgNode>,
}

fn render_element(node: &SvgNode, ctx: &mut RenderCtx) {
    let SvgNode::Element(el) = node else { return };
    match el.name.as_str() {
        "svg" => {
            // Root container: render children with inherited styles.
            with_style_scope(el, ctx, |ctx| render_children(el, ctx));
        }
        "defs" | "symbol" => {
            // Definitions are collected separately; don't render inline.
        }
        "g" => {
            let pushed = maybe_apply_transform(el, ctx);
            with_style_scope(el, ctx, |ctx| render_children(el, ctx));
            if pushed {
                ctx.pop_transform();
            }
        }
        "use" => render_use(el, ctx),
        "rect" => render_rect(el, ctx),
        "circle" => render_circle(el, ctx),
        "ellipse" => render_ellipse(el, ctx),
        "line" => render_line(el, ctx),
        "polyline" => render_polyline(el, ctx, false),
        "polygon" => render_polyline(el, ctx, true),
        "path" => render_path(el, ctx),
        "text" => render_text(el, ctx),
        _ => {
            // Unknown element: render children so nested shapes still appear.
            render_children(el, ctx);
        }
    }
}

fn render_children(el: &SvgElement, ctx: &mut RenderCtx) {
    for child in &el.children {
        render_element(child, ctx);
    }
}

fn with_style_scope<F: FnOnce(&mut RenderCtx)>(el: &SvgElement, ctx: &mut RenderCtx, f: F) {
    let saved_fill = ctx.fill.clone();
    let saved_stroke = ctx.stroke.clone();
    let saved_sw = ctx.stroke_width;
    if let Some(s) = el.attrs.get("fill")
        && let parsed = parse_paint(s)
    {
        ctx.fill = parsed;
    }
    if let Some(s) = el.attrs.get("stroke")
        && let parsed = parse_paint(s)
    {
        ctx.stroke = parsed;
    }
    if let Some(s) = el.attrs.get("stroke-width")
        && let Some(w) = s.trim().trim_end_matches("px").parse::<f32>().ok()
    {
        ctx.stroke_width = w;
    }
    f(ctx);
    ctx.fill = saved_fill;
    ctx.stroke = saved_stroke;
    ctx.stroke_width = saved_sw;
}

fn maybe_apply_transform(el: &SvgElement, ctx: &mut RenderCtx) -> bool {
    let Some(s) = el.attrs.get("transform") else {
        return false;
    };
    let m = parse_svg_transform(s);
    ctx.push_transform(m);
    true
}

/// Recursively collect elements with `id` attributes from `<defs>` and
/// `<symbol>` sections into the defs registry.
fn collect_defs(el: &SvgElement, ctx: &mut RenderCtx) {
    for child in &el.children {
        let SvgNode::Element(child_el) = child else {
            continue;
        };
        if child_el.name == "defs" || child_el.name == "symbol" {
            collect_defs_children(child_el, ctx);
        } else if child_el.name == "g" {
            // Groups can also contain id'd elements.
            collect_defs(child_el, ctx);
        }
    }
}

fn collect_defs_children(el: &SvgElement, ctx: &mut RenderCtx) {
    for child in &el.children {
        let SvgNode::Element(child_el) = child else {
            continue;
        };
        if let Some(id) = child_el.attrs.get("id") {
            ctx.defs.insert(id.clone(), child_el.clone());
        }
        // Recurse into nested groups.
        if child_el.name == "g" {
            collect_defs_children(child_el, ctx);
        }
    }
}

/// Render a `<use href="#id" x="..." y="...">` element by looking up the
/// referenced definition and rendering its children with a translate.
fn render_use(el: &SvgElement, ctx: &mut RenderCtx) {
    let href = el
        .attrs
        .get("href")
        .or_else(|| el.attrs.get("xlink:href"))
        .map(|s| s.trim_start_matches('#').to_string());
    let Some(id) = href else { return };
    let Some(def_el) = ctx.defs.get(&id).cloned() else {
        return;
    };

    let x = attr_f32(el, "x", 0.0);
    let y = attr_f32(el, "y", 0.0);

    // Apply translate for the use position, then render the referenced element's children.
    ctx.push_transform([1.0, 0.0, 0.0, 1.0, x, y]);
    let pushed_style = false;
    with_style_scope(el, ctx, |ctx| {
        for child in &def_el.children {
            render_element(child, ctx);
        }
    });
    let _ = pushed_style;
    ctx.pop_transform();
}

fn render_rect(el: &SvgElement, ctx: &mut RenderCtx) {
    let x = attr_f32(el, "x", 0.0);
    let y = attr_f32(el, "y", 0.0);
    let w = attr_f32(el, "width", 0.0);
    let h = attr_f32(el, "height", 0.0);
    let rx = attr_f32(el, "rx", 0.0);
    let ry = attr_f32(el, "ry", rx);
    let stroke = ctx.stroke_color();
    let fill = ctx.fill_color();
    let lw = ctx.line_width();
    let (x1, y1) = ctx.current().apply(x, y);
    let (x2, y2) = ctx.current().apply(x + w, y + h);
    let min_x = x1.min(x2);
    let min_y = y1.min(y2);
    let abs_w = (x2 - x1).abs();
    let abs_h = (y2 - y1).abs();
    if rx > 0.0 || ry > 0.0 {
        let r = rx.min(ry).min(abs_w / 2.0).min(abs_h / 2.0);
        let ops = rounded_rect_path_ops(min_x, min_y, abs_w, abs_h, r);
        ctx.canvas.push_shape(VectorShape::Path {
            ops,
            stroke,
            fill,
            line_width: lw,
        });
    } else {
        ctx.canvas.push_shape(VectorShape::Rect {
            x: min_x,
            y: min_y,
            width: abs_w,
            height: abs_h,
            stroke,
            fill,
            line_width: lw,
        });
    }
}

fn render_circle(el: &SvgElement, ctx: &mut RenderCtx) {
    let cx = attr_f32(el, "cx", 0.0);
    let cy = attr_f32(el, "cy", 0.0);
    let r = attr_f32(el, "r", 0.0);
    let (x, y) = ctx.current().apply(cx, cy);
    ctx.canvas.push_shape(VectorShape::Ellipse {
        cx: x,
        cy: y,
        rx: r,
        ry: r,
        stroke: ctx.stroke_color(),
        fill: ctx.fill_color(),
        line_width: ctx.line_width(),
    });
}

fn render_ellipse(el: &SvgElement, ctx: &mut RenderCtx) {
    let cx = attr_f32(el, "cx", 0.0);
    let cy = attr_f32(el, "cy", 0.0);
    let rx = attr_f32(el, "rx", 0.0);
    let ry = attr_f32(el, "ry", 0.0);
    let (x, y) = ctx.current().apply(cx, cy);
    ctx.canvas.push_shape(VectorShape::Ellipse {
        cx: x,
        cy: y,
        rx,
        ry,
        stroke: ctx.stroke_color(),
        fill: ctx.fill_color(),
        line_width: ctx.line_width(),
    });
}

fn render_line(el: &SvgElement, ctx: &mut RenderCtx) {
    let x1 = attr_f32(el, "x1", 0.0);
    let y1 = attr_f32(el, "y1", 0.0);
    let x2 = attr_f32(el, "x2", 0.0);
    let y2 = attr_f32(el, "y2", 0.0);
    let (xa, ya) = ctx.current().apply(x1, y1);
    let (xb, yb) = ctx.current().apply(x2, y2);
    let stroke = ctx.stroke_color().unwrap_or(Color::black());
    ctx.canvas.push_shape(VectorShape::Line {
        x1: xa,
        y1: ya,
        x2: xb,
        y2: yb,
        stroke,
        width: ctx.line_width(),
    });
}

fn render_polyline(el: &SvgElement, ctx: &mut RenderCtx, closed: bool) {
    let Some(s) = el.attrs.get("points") else {
        return;
    };
    let nums: Vec<f32> = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse::<f32>().ok())
        .collect();
    if nums.len() < 4 {
        return;
    }
    let mut pts: Vec<(f32, f32)> = nums
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| ctx.current().apply(c[0], c[1]))
        .collect();
    if closed
        && pts.first() != pts.last()
        && let Some(&(x0, y0)) = pts.first()
    {
        pts.push((x0, y0));
    }
    ctx.canvas.push_shape(VectorShape::Polygon {
        points: pts,
        stroke: ctx.stroke_color(),
        fill: ctx.fill_color(),
        line_width: ctx.line_width(),
    });
}

fn render_path(el: &SvgElement, ctx: &mut RenderCtx) {
    let Some(d) = el.attrs.get("d").cloned() else {
        return;
    };
    let ops = match parse_svg_path(&d) {
        Ok(o) => o,
        Err(_) => return,
    };
    let ops: Vec<PathOp> = ops
        .iter()
        .map(|op| match op {
            PathOp::MoveTo { x, y } => {
                let (x, y) = ctx.current().apply(*x, *y);
                PathOp::MoveTo { x, y }
            }
            PathOp::LineTo { x, y } => {
                let (x, y) = ctx.current().apply(*x, *y);
                PathOp::LineTo { x, y }
            }
            PathOp::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => {
                let (x1, y1) = ctx.current().apply(*x1, *y1);
                let (x2, y2) = ctx.current().apply(*x2, *y2);
                let (x3, y3) = ctx.current().apply(*x3, *y3);
                PathOp::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                }
            }
            PathOp::Close => PathOp::Close,
        })
        .collect();
    ctx.canvas.push_shape(VectorShape::Path {
        ops,
        stroke: ctx.stroke_color(),
        fill: ctx.fill_color(),
        line_width: ctx.line_width(),
    });
}

fn render_text(el: &SvgElement, ctx: &mut RenderCtx) {
    let x = attr_f32(el, "x", 0.0);
    let y = attr_f32(el, "y", 0.0);
    let (x, y) = ctx.current().apply(x, y);
    // Concatenate all descendant text.
    let mut text = String::new();
    collect_text(el, &mut text);
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    let size = el
        .attrs
        .get("font-size")
        .and_then(|s| {
            s.trim()
                .trim_end_matches("px")
                .trim_end_matches("pt")
                .parse::<f32>()
                .ok()
        })
        .unwrap_or(12.0);
    let fill = ctx.fill_color().unwrap_or(Color::black());
    ctx.canvas.push_shape(VectorShape::Text {
        x,
        y,
        text,
        size,
        fill,
    });
}

fn collect_text(el: &SvgElement, out: &mut String) {
    for child in &el.children {
        match child {
            SvgNode::Text(s) => out.push_str(s),
            SvgNode::Element(e) if matches!(e.name.as_str(), "tspan" | "a") => {
                collect_text(e, out);
            }
            _ => {}
        }
    }
}

fn apply_svg_viewbox(el: &SvgElement, ctx: &mut RenderCtx, layout: PageLayout) {
    let svg_w = attr_f32(el, "width", layout.width);
    let svg_h = attr_f32(el, "height", layout.height);
    if let Some(vb) = el.attrs.get("viewBox") {
        let nums: Vec<f32> = vb
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse::<f32>().ok())
            .collect();
        if nums.len() == 4 {
            let (vbx, vby, vbw, vbh) = (nums[0], nums[1], nums[2], nums[3]);
            if vbw > 0.0 && vbh > 0.0 {
                let sx = svg_w / vbw;
                let sy = svg_h / vbh;
                ctx.push_transform([sx, 0.0, 0.0, sy, -vbx * sx, -vby * sy]);
            }
        }
    }
}

fn rounded_rect_path_ops(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<PathOp> {
    let k = 0.552_284_8_f32 * r;
    vec![
        PathOp::MoveTo { x: x + r, y: y + h },
        PathOp::LineTo {
            x: x + w - r,
            y: y + h,
        },
        PathOp::CurveTo {
            x1: x + w - r + k,
            y1: y + h,
            x2: x + w,
            y2: y + h - r + k,
            x3: x + w,
            y3: y + h - r,
        },
        PathOp::LineTo { x: x + w, y: y + r },
        PathOp::CurveTo {
            x1: x + w,
            y1: y + r - k,
            x2: x + w - r + k,
            y2: y,
            x3: x + w - r,
            y3: y,
        },
        PathOp::LineTo { x: x + r, y },
        PathOp::CurveTo {
            x1: x + r - k,
            y1: y,
            x2: x,
            y2: y + r - k,
            x3: x,
            y3: y + r,
        },
        PathOp::LineTo { x, y: y + h - r },
        PathOp::CurveTo {
            x1: x,
            y1: y + h - r + k,
            x2: x + r - k,
            y2: y + h,
            x3: x + r,
            y3: y + h,
        },
        PathOp::Close,
    ]
}

fn attr_f32(el: &SvgElement, key: &str, default: f32) -> f32 {
    el.attrs
        .get(key)
        .and_then(|s| {
            s.trim()
                .trim_end_matches("px")
                .trim_end_matches("pt")
                .parse::<f32>()
                .ok()
        })
        .unwrap_or(default)
}

fn parse_paint(s: &str) -> SvgPaint {
    let trimmed = s.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "none" => SvgPaint::None,
        "inherit" | "currentcolor" | "" => SvgPaint::Inherit,
        "black" => SvgPaint::Color(Color::black()),
        "white" => SvgPaint::Color(Color::rgb(1.0, 1.0, 1.0)),
        "red" => SvgPaint::Color(Color::rgb(1.0, 0.0, 0.0)),
        "green" => SvgPaint::Color(Color::rgb(0.0, 0.5, 0.0)),
        "blue" => SvgPaint::Color(Color::rgb(0.0, 0.0, 1.0)),
        "yellow" => SvgPaint::Color(Color::rgb(1.0, 1.0, 0.0)),
        "cyan" => SvgPaint::Color(Color::rgb(0.0, 1.0, 1.0)),
        "magenta" => SvgPaint::Color(Color::rgb(1.0, 0.0, 1.0)),
        "gray" | "grey" => SvgPaint::Color(Color::rgb(0.5, 0.5, 0.5)),
        _ if trimmed.starts_with('#') => {
            if let Some(c) = parse_hex_color(trimmed) {
                SvgPaint::Color(c)
            } else {
                SvgPaint::Color(Color::black())
            }
        }
        _ if trimmed.starts_with("rgb(") && trimmed.ends_with(')') => {
            let inner = &trimmed[4..trimmed.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 3
                && let (Ok(r), Ok(g), Ok(b)) = (
                    parts[0].parse::<u8>(),
                    parts[1].parse::<u8>(),
                    parts[2].parse::<u8>(),
                )
            {
                SvgPaint::Color(Color::rgb(
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    b as f32 / 255.0,
                ))
            } else {
                SvgPaint::Color(Color::black())
            }
        }
        _ => SvgPaint::Color(Color::black()),
    }
}

fn parse_hex_color(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#')?;
    let (r, g, b) = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            (r, g, b)
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b)
        }
        _ => return None,
    };
    Some(Color::rgb(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::validate_pdf_bytes;

    #[test]
    fn test_svg_document_rects_and_groups() {
        let svg = r##"
            <svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
              <g transform="translate(50,50)">
                <rect x="0" y="0" width="100" height="60" fill="#ff0000" stroke="#000000" stroke-width="2"/>
              </g>
              <rect x="10" y="10" width="20" height="20" fill="blue"/>
            </svg>"##;
        let canvas = parse_svg_document(svg, PageLayout::portrait()).unwrap();
        assert!(
            canvas.shapes().len() >= 2,
            "expected >=2 shapes, got {}",
            canvas.shapes().len()
        );
    }

    #[test]
    fn test_svg_paint_hex_and_named() {
        assert!(matches!(parse_paint("#ff0000"), SvgPaint::Color(_)));
        assert!(matches!(parse_paint("red"), SvgPaint::Color(_)));
        assert!(matches!(parse_paint("none"), SvgPaint::None));
        assert!(matches!(parse_paint("inherit"), SvgPaint::Inherit));
    }

    #[test]
    fn test_svg_document_circle_and_line() {
        let svg = r##"
            <svg width="200" height="200">
              <circle cx="50" cy="50" r="20" fill="green"/>
              <line x1="0" y1="0" x2="100" y2="100" stroke="black"/>
            </svg>"##;
        let canvas = parse_svg_document(svg, PageLayout::portrait()).unwrap();
        assert!(canvas.shapes().len() >= 2);
    }

    #[test]
    fn test_svg_document_polygon() {
        let svg = r##"
            <svg width="200" height="200">
              <polygon points="0,0 100,0 100,100 0,100" fill="blue"/>
              <polyline points="10,10 50,10 50,50" stroke="black"/>
            </svg>"##;
        let canvas = parse_svg_document(svg, PageLayout::portrait()).unwrap();
        assert!(canvas.shapes().len() >= 2);
    }

    #[test]
    fn test_svg_document_to_pdf_bytes() {
        let svg = r##"
            <svg width="200" height="200">
              <rect x="10" y="10" width="80" height="80" fill="#336699" stroke="black"/>
            </svg>"##;
        let bytes = svg_document_to_pdf_bytes(svg, PageLayout::portrait()).unwrap();
        let validation = validate_pdf_bytes(&bytes);
        assert!(validation.valid, "{:?}", validation.errors);
    }

    #[test]
    fn test_svg_path_element_still_works() {
        let svg = r##"
            <svg>
              <path d="M10 10 L100 10 L55 90 Z" fill="#abcdef"/>
            </svg>"##;
        let canvas = parse_svg_document(svg, PageLayout::portrait()).unwrap();
        assert!(!canvas.shapes().is_empty());
    }

    #[test]
    fn test_svg_default_fill_is_black() {
        let svg = r##"<svg width="100" height="100"><rect width="50" height="50"/></svg>"##;
        let canvas = parse_svg_document(svg, PageLayout::portrait()).unwrap();
        assert!(!canvas.shapes().is_empty());
        match &canvas.shapes()[0] {
            VectorShape::Rect { fill, .. } => {
                assert!(fill.is_some(), "default fill should be black, not none");
            }
            _ => panic!("expected Rect"),
        }
    }

    #[test]
    fn test_svg_default_stroke_is_none() {
        let svg =
            r##"<svg width="100" height="100"><rect width="50" height="50" fill="red"/></svg>"##;
        let canvas = parse_svg_document(svg, PageLayout::portrait()).unwrap();
        match &canvas.shapes()[0] {
            VectorShape::Rect { stroke, .. } => {
                assert!(stroke.is_none(), "default stroke should be none, not black");
            }
            _ => panic!("expected Rect"),
        }
    }

    #[test]
    fn test_svg_viewbox_scaling() {
        let svg = r##"<svg width="400" height="300" viewBox="0 0 200 150"><rect width="100" height="75" fill="blue"/></svg>"##;
        let canvas = parse_svg_document(svg, PageLayout::portrait()).unwrap();
        assert!(!canvas.shapes().is_empty());
    }

    #[test]
    fn test_svg_rounded_rect() {
        let svg = r##"<svg width="200" height="200"><rect x="10" y="10" width="80" height="60" rx="10" ry="10" fill="red"/></svg>"##;
        let canvas = parse_svg_document(svg, PageLayout::portrait()).unwrap();
        assert!(!canvas.shapes().is_empty());
        match &canvas.shapes()[0] {
            VectorShape::Path { ops, .. } => {
                assert!(
                    ops.len() > 4,
                    "rounded rect should produce a path with curves"
                );
            }
            VectorShape::Rect { .. } => panic!("rounded rect should be a Path, not Rect"),
            _ => panic!("unexpected shape"),
        }
    }

    #[test]
    fn test_svg_text_renders_as_text_shape() {
        let svg = r##"<svg width="200" height="100"><text x="50" y="50" font-size="14" fill="black">Hello</text></svg>"##;
        let canvas = parse_svg_document(svg, PageLayout::portrait()).unwrap();
        let has_text = canvas
            .shapes()
            .iter()
            .any(|s| matches!(s, VectorShape::Text { .. }));
        assert!(has_text, "should have a Text shape, not a Rect");
    }

    #[test]
    fn test_svg_text_pdf_contains_text_operators() {
        let svg = r##"<svg width="200" height="100"><text x="50" y="50" font-size="14" fill="black">Hello</text></svg>"##;
        let bytes = svg_document_to_pdf_bytes(svg, PageLayout::portrait()).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.contains("BT"),
            "PDF should contain BT operator for text"
        );
        assert!(
            text.contains("Tj"),
            "PDF should contain Tj operator for text"
        );
        assert!(text.contains("/F1"), "PDF should reference /F1 font");
        assert!(
            text.contains("Helvetica"),
            "PDF should embed Helvetica font"
        );
    }

    #[test]
    fn test_svg_use_element() {
        let svg = r##"<svg width="200" height="200" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <g id="mybox">
              <rect width="40" height="40" fill="blue"/>
            </g>
          </defs>
          <use href="#mybox" x="10" y="10"/>
          <use href="#mybox" x="60" y="60"/>
        </svg>"##;
        let canvas = parse_svg_document(svg, PageLayout::portrait()).unwrap();
        assert!(
            canvas.shapes().len() >= 2,
            "use element should produce shapes, got {}",
            canvas.shapes().len()
        );
    }

    #[test]
    fn test_svg_cdata_section() {
        let svg = r##"<svg width="200" height="100">
          <text x="10" y="50"><![CDATA[CDATA text]]></text>
        </svg>"##;
        let canvas = parse_svg_document(svg, PageLayout::portrait()).unwrap();
        let has_text = canvas
            .shapes()
            .iter()
            .any(|s| matches!(s, VectorShape::Text { text, .. } if text.contains("CDATA text")));
        assert!(has_text, "should render text from CDATA section");
    }

    #[test]
    fn test_svg_document_valid_pdf_with_text() {
        let svg = r##"<svg width="300" height="200" xmlns="http://www.w3.org/2000/svg">
          <rect x="10" y="10" width="100" height="60" fill="#336699" stroke="black"/>
          <text x="20" y="40" font-size="16" fill="white">Label</text>
          <circle cx="200" cy="100" r="30" fill="red"/>
        </svg>"##;
        let bytes = svg_document_to_pdf_bytes(svg, PageLayout::portrait()).unwrap();
        let validation = validate_pdf_bytes(&bytes);
        assert!(validation.valid, "{:?}", validation.errors);
    }
}
