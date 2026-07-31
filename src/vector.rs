//! Vector graphics for PDF content streams
//!
//! Provides path-based drawing primitives (lines, rectangles, ellipses, polygons,
//! and cubic Bézier paths) that compile to standard PDF operators (`m`, `l`, `c`,
//! `re`, `S`, `f`, `B`, etc.).

use crate::pdf_generator::{Color, PageLayout, PdfGenerator};
use anyhow::Result;
use std::collections::HashMap;

/// How a path is painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintMode {
    Stroke,
    Fill,
    FillAndStroke,
}

/// A single path construction command.
#[derive(Debug, Clone, PartialEq)]
pub enum PathOp {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    CurveTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    },
    Close,
}

/// A drawable vector shape.
#[derive(Debug, Clone, PartialEq)]
pub enum VectorShape {
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        stroke: Color,
        width: f32,
    },
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        stroke: Option<Color>,
        fill: Option<Color>,
        line_width: f32,
    },
    Ellipse {
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        stroke: Option<Color>,
        fill: Option<Color>,
        line_width: f32,
    },
    Polygon {
        points: Vec<(f32, f32)>,
        stroke: Option<Color>,
        fill: Option<Color>,
        line_width: f32,
    },
    Path {
        ops: Vec<PathOp>,
        stroke: Option<Color>,
        fill: Option<Color>,
        line_width: f32,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        fill: Color,
    },
}

/// Accumulates vector shapes and emits a PDF content stream.
#[derive(Debug, Clone, Default)]
pub struct VectorCanvas {
    shapes: Vec<VectorShape>,
}

impl VectorCanvas {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shapes(&self) -> &[VectorShape] {
        &self.shapes
    }

    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, shape: VectorShape) -> Self {
        self.shapes.push(shape);
        self
    }

    /// Push a shape onto the canvas in place (used by the SVG renderer).
    pub fn push_shape(&mut self, shape: VectorShape) {
        self.shapes.push(shape);
    }

    pub fn line(self, x1: f32, y1: f32, x2: f32, y2: f32, stroke: Color, width: f32) -> Self {
        self.add(VectorShape::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            width,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rect(
        self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        stroke: Option<Color>,
        fill: Option<Color>,
        line_width: f32,
    ) -> Self {
        self.add(VectorShape::Rect {
            x,
            y,
            width,
            height,
            stroke,
            fill,
            line_width,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(
        self,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        stroke: Option<Color>,
        fill: Option<Color>,
        line_width: f32,
    ) -> Self {
        self.add(VectorShape::Ellipse {
            cx,
            cy,
            rx,
            ry,
            stroke,
            fill,
            line_width,
        })
    }

    pub fn polygon(
        self,
        points: Vec<(f32, f32)>,
        stroke: Option<Color>,
        fill: Option<Color>,
        line_width: f32,
    ) -> Self {
        self.add(VectorShape::Polygon {
            points,
            stroke,
            fill,
            line_width,
        })
    }

    /// Add an SVG path `d` attribute as a drawable path.
    pub fn svg_path(
        self,
        d: &str,
        stroke: Option<Color>,
        fill: Option<Color>,
        line_width: f32,
    ) -> Result<Self> {
        let ops = parse_svg_path(d)?;
        Ok(self.add(VectorShape::Path {
            ops,
            stroke,
            fill,
            line_width,
        }))
    }

    /// Emit PDF content-stream operators for all shapes.
    pub fn to_content_stream(&self) -> String {
        let mut out = String::new();
        for shape in &self.shapes {
            out.push_str(&shape_to_ops(shape));
        }
        out
    }

    /// Write a one-page PDF containing the canvas drawings.
    pub fn write_pdf(&self, path: &str, layout: PageLayout) -> Result<()> {
        let bytes = self.to_pdf_bytes(layout)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Generate PDF bytes for a single page with the canvas drawings.
    pub fn to_pdf_bytes(&self, layout: PageLayout) -> Result<Vec<u8>> {
        let content = self.to_content_stream();
        let content_bytes = content.into_bytes();

        let mut generator = PdfGenerator::new().with_version(layout.version);

        let content_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", content_bytes.len()),
            content_bytes,
        );

        // Add a Helvetica font so SVG <text> elements can render.
        let font_id = generator.add_object(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n".to_string(),
        );

        // page will be next_id, pages the one after that
        let pages_id = generator.next_id + 1;

        let page_dict = format!(
            "<< /Type /Page\n\
             /Parent {} 0 R\n\
             /MediaBox [0 0 {} {}]\n\
             /Contents {} 0 R\n\
             /Resources << /Font << /F1 {} 0 R >> >>\n\
             >>\n",
            pages_id, layout.width, layout.height, content_id, font_id
        );
        let page_id = generator.add_object(page_dict);

        let pages_dict = format!("<< /Type /Pages\n/Kids [{} 0 R]\n/Count 1\n>>\n", page_id);
        let actual_pages_id = generator.add_object(pages_dict);
        assert_eq!(actual_pages_id, pages_id);

        let catalog = format!("<< /Type /Catalog\n/Pages {} 0 R\n>>\n", actual_pages_id);
        generator.add_object(catalog);

        Ok(generator.generate())
    }
}

/// Parse an SVG path `d` attribute into PDF path operators.
///
/// Supports `M/m`, `L/l`, `H/h`, `V/v`, `C/c`, `S/s`, `Q/q`, `T/t`, and `Z/z`.
/// Arc commands (`A/a`) are not supported.
pub fn parse_svg_path(d: &str) -> Result<Vec<PathOp>> {
    let tokens = tokenize_svg_path(d)?;
    let mut ops = Vec::new();
    let mut i = 0usize;
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let mut start_x = 0.0f32;
    let mut start_y = 0.0f32;
    let mut last_cmd = ' ';
    let mut last_ctrl: Option<(f32, f32)> = None; // for S/T reflection

    while i < tokens.len() {
        let cmd = match &tokens[i] {
            SvgToken::Command(c) => {
                i += 1;
                *c
            }
            SvgToken::Number(_) => {
                // Implicit command repetition
                match last_cmd {
                    'M' => 'L',
                    'm' => 'l',
                    c if c.is_ascii_alphabetic() => c,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "SVG path number without a preceding command near token {i}"
                        ));
                    }
                }
            }
        };

        let relative = cmd.is_ascii_lowercase();
        let cmd_upper = cmd.to_ascii_uppercase();

        match cmd_upper {
            'M' => {
                let (x, y) = read_pair(&tokens, &mut i)?;
                let (x, y) = if relative { (cx + x, cy + y) } else { (x, y) };
                ops.push(PathOp::MoveTo { x, y });
                cx = x;
                cy = y;
                start_x = x;
                start_y = y;
                last_ctrl = None;
                last_cmd = if relative { 'm' } else { 'M' };
                // Subsequent coordinate pairs are implicit LineTo
                while i < tokens.len() && matches!(tokens[i], SvgToken::Number(_)) {
                    let (x, y) = read_pair(&tokens, &mut i)?;
                    let (x, y) = if relative { (cx + x, cy + y) } else { (x, y) };
                    ops.push(PathOp::LineTo { x, y });
                    cx = x;
                    cy = y;
                    last_cmd = if relative { 'l' } else { 'L' };
                }
            }
            'L' => {
                let (x, y) = read_pair(&tokens, &mut i)?;
                let (x, y) = if relative { (cx + x, cy + y) } else { (x, y) };
                ops.push(PathOp::LineTo { x, y });
                cx = x;
                cy = y;
                last_ctrl = None;
                last_cmd = cmd;
            }
            'H' => {
                let x = read_num(&tokens, &mut i)?;
                let x = if relative { cx + x } else { x };
                ops.push(PathOp::LineTo { x, y: cy });
                cx = x;
                last_ctrl = None;
                last_cmd = cmd;
            }
            'V' => {
                let y = read_num(&tokens, &mut i)?;
                let y = if relative { cy + y } else { y };
                ops.push(PathOp::LineTo { x: cx, y });
                cy = y;
                last_ctrl = None;
                last_cmd = cmd;
            }
            'C' => {
                let (x1, y1) = read_pair(&tokens, &mut i)?;
                let (x2, y2) = read_pair(&tokens, &mut i)?;
                let (x3, y3) = read_pair(&tokens, &mut i)?;
                let (x1, y1, x2, y2, x3, y3) = if relative {
                    (cx + x1, cy + y1, cx + x2, cy + y2, cx + x3, cy + y3)
                } else {
                    (x1, y1, x2, y2, x3, y3)
                };
                ops.push(PathOp::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                });
                last_ctrl = Some((x2, y2));
                cx = x3;
                cy = y3;
                last_cmd = cmd;
            }
            'S' => {
                let (x2, y2) = read_pair(&tokens, &mut i)?;
                let (x3, y3) = read_pair(&tokens, &mut i)?;
                let (x2, y2, x3, y3) = if relative {
                    (cx + x2, cy + y2, cx + x3, cy + y3)
                } else {
                    (x2, y2, x3, y3)
                };
                let (x1, y1) = match (last_cmd.to_ascii_uppercase(), last_ctrl) {
                    ('C' | 'S', Some((px, py))) => (2.0 * cx - px, 2.0 * cy - py),
                    _ => (cx, cy),
                };
                ops.push(PathOp::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                });
                last_ctrl = Some((x2, y2));
                cx = x3;
                cy = y3;
                last_cmd = cmd;
            }
            'Q' => {
                let (qx, qy) = read_pair(&tokens, &mut i)?;
                let (x3, y3) = read_pair(&tokens, &mut i)?;
                let (qx, qy, x3, y3) = if relative {
                    (cx + qx, cy + qy, cx + x3, cy + y3)
                } else {
                    (qx, qy, x3, y3)
                };
                let (x1, y1, x2, y2) = quad_to_cubic(cx, cy, qx, qy, x3, y3);
                ops.push(PathOp::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                });
                last_ctrl = Some((qx, qy));
                cx = x3;
                cy = y3;
                last_cmd = cmd;
            }
            'T' => {
                let (x3, y3) = read_pair(&tokens, &mut i)?;
                let (x3, y3) = if relative {
                    (cx + x3, cy + y3)
                } else {
                    (x3, y3)
                };
                let (qx, qy) = match (last_cmd.to_ascii_uppercase(), last_ctrl) {
                    ('Q' | 'T', Some((px, py))) => (2.0 * cx - px, 2.0 * cy - py),
                    _ => (cx, cy),
                };
                let (x1, y1, x2, y2) = quad_to_cubic(cx, cy, qx, qy, x3, y3);
                ops.push(PathOp::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                });
                last_ctrl = Some((qx, qy));
                cx = x3;
                cy = y3;
                last_cmd = cmd;
            }
            'Z' => {
                ops.push(PathOp::Close);
                cx = start_x;
                cy = start_y;
                last_ctrl = None;
                last_cmd = cmd;
            }
            'A' => {
                return Err(anyhow::anyhow!(
                    "SVG arc commands (A/a) are not supported; convert arcs to cubics first"
                ));
            }
            other => {
                return Err(anyhow::anyhow!("Unsupported SVG path command '{other}'"));
            }
        }
    }

    Ok(ops)
}

/// Extract the first `d="..."` path from a simple SVG document string.
pub fn extract_svg_path_d(svg: &str) -> Result<String> {
    // Prefer path elements; fall back to any d="..."
    let re = regex::Regex::new(r#"(?i)<path\b[^>]*\bd\s*=\s*["']([^"']+)["']"#).unwrap();
    if let Some(caps) = re.captures(svg) {
        return Ok(caps[1].to_string());
    }
    let re_any = regex::Regex::new(r#"(?i)\bd\s*=\s*["']([^"']+)["']"#).unwrap();
    if let Some(caps) = re_any.captures(svg) {
        return Ok(caps[1].to_string());
    }
    Err(anyhow::anyhow!("No SVG path d= attribute found"))
}

/// Create a one-page PDF from an SVG path `d` string.
pub fn svg_path_to_pdf_bytes(
    d: &str,
    layout: PageLayout,
    stroke: Option<Color>,
    fill: Option<Color>,
    line_width: f32,
) -> Result<Vec<u8>> {
    VectorCanvas::new()
        .svg_path(d, stroke, fill, line_width)?
        .to_pdf_bytes(layout)
}

/// Create a one-page PDF from an SVG file (uses the first `<path d="...">`).
pub fn svg_file_to_pdf(
    svg_path: &str,
    output_pdf: &str,
    layout: PageLayout,
    stroke: Option<Color>,
    fill: Option<Color>,
    line_width: f32,
) -> Result<()> {
    let svg = std::fs::read_to_string(svg_path)?;
    let d = extract_svg_path_d(&svg)?;
    let bytes = svg_path_to_pdf_bytes(&d, layout, stroke, fill, line_width)?;
    std::fs::write(output_pdf, bytes)?;
    Ok(())
}

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
enum SvgNode {
    Element(SvgElement),
    Text(String),
}

#[derive(Debug, Clone)]
struct SvgElement {
    name: String,
    attrs: HashMap<String, String>,
    children: Vec<SvgNode>,
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
        let SvgNode::Element(child_el) = child else { continue };
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
        let SvgNode::Element(child_el) = child else { continue };
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
    let Some(def_el) = ctx.defs.get(&id).cloned() else { return };

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
        ctx.canvas.push_shape(VectorShape::Path { ops, stroke, fill, line_width: lw });
    } else {
        ctx.canvas.push_shape(VectorShape::Rect {
            x: min_x, y: min_y, width: abs_w, height: abs_h, stroke, fill, line_width: lw,
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
        let nums: Vec<f32> = vb.split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse::<f32>().ok()).collect();
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
        PathOp::LineTo { x: x + w - r, y: y + h },
        PathOp::CurveTo { x1: x + w - r + k, y1: y + h, x2: x + w, y2: y + h - r + k, x3: x + w, y3: y + h - r },
        PathOp::LineTo { x: x + w, y: y + r },
        PathOp::CurveTo { x1: x + w, y1: y + r - k, x2: x + w - r + k, y2: y, x3: x + w - r, y3: y },
        PathOp::LineTo { x: x + r, y },
        PathOp::CurveTo { x1: x + r - k, y1: y, x2: x, y2: y + r - k, x3: x, y3: y + r },
        PathOp::LineTo { x, y: y + h - r },
        PathOp::CurveTo { x1: x, y1: y + h - r + k, x2: x + r - k, y2: y + h, x3: x + r, y3: y + h },
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

/// Parse `transform="translate(x,y) rotate(a) scale(s) matrix(a,b,c,d,e,f)"`.
pub fn parse_svg_transform(s: &str) -> [f32; 6] {
    let mut result = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let re = regex::Regex::new(r"(?i)(translate|rotate|scale|matrix|skewx|skewy)\s*\(([^)]*)\)")
        .unwrap();
    for caps in re.captures_iter(s) {
        let func = caps[1].to_ascii_lowercase();
        let args: Vec<f32> = caps[2]
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse::<f32>().ok())
            .collect();
        let next = match func.as_str() {
            "translate" => {
                let tx = args.first().copied().unwrap_or(0.0);
                let ty = args.get(1).copied().unwrap_or(0.0);
                [1.0, 0.0, 0.0, 1.0, tx, ty]
            }
            "scale" => {
                let sx = args.first().copied().unwrap_or(1.0);
                let sy = args.get(1).copied().unwrap_or(sx);
                [sx, 0.0, 0.0, sy, 0.0, 0.0]
            }
            "rotate" => {
                let a = args.first().copied().unwrap_or(0.0).to_radians();
                let cos = a.cos();
                let sin = a.sin();
                if args.len() >= 3 {
                    let cx = args[1];
                    let cy = args[2];
                    // T(cx,cy) * R(a) * T(-cx,-cy)
                    let m1 = [1.0, 0.0, 0.0, 1.0, cx, cy];
                    let m2 = [cos, sin, -sin, cos, 0.0, 0.0];
                    let m3 = [1.0, 0.0, 0.0, 1.0, -cx, -cy];
                    compose(&compose(&m1, &m2), &m3)
                } else {
                    [cos, sin, -sin, cos, 0.0, 0.0]
                }
            }
            "matrix" if args.len() == 6 => [args[0], args[1], args[2], args[3], args[4], args[5]],
            "skewx" => {
                let a = args.first().copied().unwrap_or(0.0).to_radians();
                [1.0, 0.0, a.tan(), 1.0, 0.0, 0.0]
            }
            "skewy" => {
                let a = args.first().copied().unwrap_or(0.0).to_radians();
                [1.0, a.tan(), 0.0, 1.0, 0.0, 0.0]
            }
            _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        };
        result = compose(&result, &next);
    }
    result
}

fn compose(a: &[f32; 6], b: &[f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

// ----- Minimal XML parser for SVG ----------------------------------------

fn parse_svg_xml(src: &str) -> Result<SvgElement> {
    let mut parser = SvgXmlParser::new(src);
    parser.skip_prolog();
    let root = parser.parse_element()?;
    Ok(root)
}

struct SvgXmlParser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> SvgXmlParser<'a> {
    fn new(src: &'a str) -> Self {
        SvgXmlParser { src, pos: 0 }
    }

    fn skip_prolog(&mut self) {
        self.skip_ws();
        if self.src[self.pos..].starts_with("<?xml")
            && let Some(end) = self.src[self.pos..].find("?>")
        {
            self.pos += end + 2;
        }
        if self.src[self.pos..].starts_with("<!--")
            && let Some(end) = self.src[self.pos..].find("-->")
        {
            self.pos += end + 3;
        }
        // Skip DOCTYPE if present.
        if self.src[self.pos..]
            .to_ascii_uppercase()
            .starts_with("<!doctype")
            && let Some(end) = self.src[self.pos..].find('>')
        {
            self.pos += end + 1;
        }
    }

    fn skip_ws(&mut self) {
        let bytes = self.src.as_bytes();
        while self.pos < bytes.len() {
            let c = bytes[self.pos] as char;
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn parse_element(&mut self) -> Result<SvgElement> {
        self.skip_ws();
        let bytes = self.src.as_bytes();
        if self.pos >= bytes.len() || bytes[self.pos] as char != '<' {
            return Err(anyhow::anyhow!("expected '<' at element start"));
        }
        self.pos += 1; // consume '<'
        // Self-closing or end tag.
        if self.peek() == Some('/') {
            return Err(anyhow::anyhow!("unexpected closing tag"));
        }
        let name = self.parse_name();
        let mut attrs = HashMap::new();
        loop {
            self.skip_ws();
            if self.peek() == Some('/') {
                self.pos += 1;
                if self.peek() == Some('>') {
                    self.pos += 1;
                }
                return Ok(SvgElement {
                    name,
                    attrs,
                    children: Vec::new(),
                });
            }
            if self.peek() == Some('>') {
                self.pos += 1;
                break;
            }
            // Parse attribute.
            let key = self.parse_name();
            if key.is_empty() {
                // Skip unrecognized character.
                self.pos += 1;
                continue;
            }
            self.skip_ws();
            if self.peek() == Some('=') {
                self.pos += 1;
                self.skip_ws();
                let value = self.parse_attr_value();
                attrs.insert(key, value);
            } else {
                attrs.insert(key.clone(), String::new());
            }
        }
        // Parse children until matching close tag.
        let mut children = Vec::new();
        loop {
            self.skip_ws();
            if self.pos >= self.src.len() {
                break;
            }
            let rest = &self.src[self.pos..];
            if rest.starts_with("</") {
                // Closing tag.
                self.pos += 2;
                let close_name = self.parse_name();
                self.skip_ws();
                if self.peek() == Some('>') {
                    self.pos += 1;
                }
                let _ = close_name; // best-effort: don't strictly verify name
                break;
            }
            if rest.starts_with("<!--") {
                if let Some(end) = rest.find("-->") {
                    self.pos += end + 3;
                    continue;
                } else {
                    break;
                }
            }
            if rest.starts_with("<![CDATA[") {
                if let Some(end) = rest.find("]]>") {
                    let text = &rest[9..end];
                    if !text.trim().is_empty() {
                        children.push(SvgNode::Text(text.to_string()));
                    }
                    self.pos += end + 3;
                    continue;
                } else {
                    break;
                }
            }
            if rest.starts_with('<') {
                let child = self.parse_element();
                if let Ok(c) = child {
                    children.push(SvgNode::Element(c));
                } else {
                    // Skip malformed element.
                    if let Some(end) = rest.find('>') {
                        self.pos += end + 1;
                    } else {
                        break;
                    }
                }
            } else {
                // Text node — collect until next '<'.
                let end = rest.find('<').unwrap_or(rest.len());
                let text = &rest[..end];
                if !text.trim().is_empty() {
                    children.push(SvgNode::Text(decode_entities(text)));
                }
                self.pos += end;
            }
        }
        Ok(SvgElement {
            name,
            attrs,
            children,
        })
    }

    fn parse_name(&mut self) -> String {
        let bytes = self.src.as_bytes();
        let start = self.pos;
        while self.pos < bytes.len() {
            let c = bytes[self.pos] as char;
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ':' || c == '.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.src[start..self.pos].to_string()
    }

    fn parse_attr_value(&mut self) -> String {
        let bytes = self.src.as_bytes();
        if self.pos >= bytes.len() {
            return String::new();
        }
        let quote = bytes[self.pos] as char;
        if quote != '"' && quote != '\'' {
            // Unquoted value until whitespace or '>'.
            let start = self.pos;
            while self.pos < bytes.len() {
                let c = bytes[self.pos] as char;
                if c.is_whitespace() || c == '>' || c == '/' {
                    break;
                }
                self.pos += 1;
            }
            return self.src[start..self.pos].to_string();
        }
        self.pos += 1;
        let start = self.pos;
        while self.pos < bytes.len() && bytes[self.pos] as char != quote {
            self.pos += 1;
        }
        let value = &self.src[start..self.pos];
        if self.pos < bytes.len() {
            self.pos += 1; // closing quote
        }
        decode_entities(value)
    }
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

#[derive(Debug)]
enum SvgToken {
    Command(char),
    Number(f32),
}

fn tokenize_svg_path(d: &str) -> Result<Vec<SvgToken>> {
    let mut tokens = Vec::new();
    let bytes = d.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() || c == ',' {
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() {
            tokens.push(SvgToken::Command(c));
            i += 1;
            continue;
        }
        if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() {
            let start = i;
            if c == '-' || c == '+' {
                i += 1;
            }
            let mut seen_dot = false;
            let mut seen_exp = false;
            while i < bytes.len() {
                let ch = bytes[i] as char;
                if ch.is_ascii_digit() {
                    i += 1;
                } else if ch == '.' && !seen_dot && !seen_exp {
                    seen_dot = true;
                    i += 1;
                } else if (ch == 'e' || ch == 'E') && !seen_exp {
                    seen_exp = true;
                    i += 1;
                    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            let num_str = std::str::from_utf8(&bytes[start..i])
                .map_err(|_| anyhow::anyhow!("Invalid UTF-8 in SVG path number"))?;
            let value: f32 = num_str
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid SVG path number '{num_str}'"))?;
            tokens.push(SvgToken::Number(value));
            continue;
        }
        return Err(anyhow::anyhow!("Unexpected character '{c}' in SVG path"));
    }
    Ok(tokens)
}

fn read_num(tokens: &[SvgToken], i: &mut usize) -> Result<f32> {
    match tokens.get(*i) {
        Some(SvgToken::Number(n)) => {
            *i += 1;
            Ok(*n)
        }
        _ => Err(anyhow::anyhow!("Expected number in SVG path at token {i}")),
    }
}

fn read_pair(tokens: &[SvgToken], i: &mut usize) -> Result<(f32, f32)> {
    let x = read_num(tokens, i)?;
    let y = read_num(tokens, i)?;
    Ok((x, y))
}

fn quad_to_cubic(x0: f32, y0: f32, qx: f32, qy: f32, x3: f32, y3: f32) -> (f32, f32, f32, f32) {
    let x1 = x0 + 2.0 / 3.0 * (qx - x0);
    let y1 = y0 + 2.0 / 3.0 * (qy - y0);
    let x2 = x3 + 2.0 / 3.0 * (qx - x3);
    let y2 = y3 + 2.0 / 3.0 * (qy - y3);
    (x1, y1, x2, y2)
}

/// Sample diagram used by the CLI `--demo` mode.
pub fn demo_canvas() -> VectorCanvas {
    VectorCanvas::new()
        .rect(
            72.0,
            600.0,
            200.0,
            120.0,
            Some(Color::rgb(0.1, 0.2, 0.6)),
            Some(Color::rgb(0.8, 0.85, 1.0)),
            2.0,
        )
        .ellipse(
            400.0,
            660.0,
            80.0,
            50.0,
            Some(Color::rgb(0.6, 0.1, 0.1)),
            Some(Color::rgb(1.0, 0.85, 0.85)),
            1.5,
        )
        .line(72.0, 560.0, 540.0, 560.0, Color::black(), 1.0)
        .polygon(
            vec![
                (150.0, 480.0),
                (220.0, 520.0),
                (290.0, 480.0),
                (260.0, 420.0),
                (180.0, 420.0),
            ],
            Some(Color::rgb(0.1, 0.5, 0.2)),
            Some(Color::rgb(0.75, 0.95, 0.8)),
            1.5,
        )
        .add(VectorShape::Path {
            ops: vec![
                PathOp::MoveTo { x: 350.0, y: 420.0 },
                PathOp::CurveTo {
                    x1: 380.0,
                    y1: 520.0,
                    x2: 480.0,
                    y2: 520.0,
                    x3: 510.0,
                    y3: 420.0,
                },
                PathOp::LineTo { x: 430.0, y: 380.0 },
                PathOp::Close,
            ],
            stroke: Some(Color::rgb(0.3, 0.0, 0.5)),
            fill: Some(Color::rgb(0.9, 0.85, 1.0)),
            line_width: 2.0,
        })
}

fn shape_to_ops(shape: &VectorShape) -> String {
    match shape {
        VectorShape::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            width,
        } => format!(
            "q\n{} w\n{} RG\n{} {} m\n{} {} l\nS\nQ\n",
            fmt(*width),
            color_ops(stroke),
            fmt(*x1),
            fmt(*y1),
            fmt(*x2),
            fmt(*y2)
        ),
        VectorShape::Rect {
            x,
            y,
            width,
            height,
            stroke,
            fill,
            line_width,
        } => {
            let mut s = String::from("q\n");
            s.push_str(&format!("{} w\n", fmt(*line_width)));
            if let Some(fill) = fill {
                s.push_str(&format!("{} rg\n", color_ops(fill)));
            }
            if let Some(stroke) = stroke {
                s.push_str(&format!("{} RG\n", color_ops(stroke)));
            }
            s.push_str(&format!(
                "{} {} {} {} re\n{}\nQ\n",
                fmt(*x),
                fmt(*y),
                fmt(*width),
                fmt(*height),
                paint_op(stroke.is_some(), fill.is_some())
            ));
            s
        }
        VectorShape::Ellipse {
            cx,
            cy,
            rx,
            ry,
            stroke,
            fill,
            line_width,
        } => {
            let ops = ellipse_path_ops(*cx, *cy, *rx, *ry);
            paint_path(&ops, *stroke, *fill, *line_width)
        }
        VectorShape::Polygon {
            points,
            stroke,
            fill,
            line_width,
        } => {
            if points.is_empty() {
                return String::new();
            }
            let mut ops = Vec::with_capacity(points.len() + 1);
            ops.push(PathOp::MoveTo {
                x: points[0].0,
                y: points[0].1,
            });
            for p in points.iter().skip(1) {
                ops.push(PathOp::LineTo { x: p.0, y: p.1 });
            }
            ops.push(PathOp::Close);
            paint_path(&ops, *stroke, *fill, *line_width)
        }
        VectorShape::Path {
            ops,
            stroke,
            fill,
            line_width,
        } => paint_path(ops, *stroke, *fill, *line_width),
        VectorShape::Text {
            x,
            y,
            text,
            size,
            fill,
        } => {
            let escaped = escape_pdf_text(text);
            format!(
                "q\nBT\n/F1 {} Tf\n{} rg\n1 0 0 1 {} {} Tm\n({}) Tj\nET\nQ\n",
                fmt(*size),
                color_ops(fill),
                fmt(*x),
                fmt(*y),
                escaped
            )
        }
    }
}

fn paint_path(
    ops: &[PathOp],
    stroke: Option<Color>,
    fill: Option<Color>,
    line_width: f32,
) -> String {
    let mut s = String::from("q\n");
    s.push_str(&format!("{} w\n", fmt(line_width)));
    if let Some(fill) = fill {
        s.push_str(&format!("{} rg\n", color_ops(&fill)));
    }
    if let Some(stroke) = stroke {
        s.push_str(&format!("{} RG\n", color_ops(&stroke)));
    }
    for op in ops {
        match op {
            PathOp::MoveTo { x, y } => s.push_str(&format!("{} {} m\n", fmt(*x), fmt(*y))),
            PathOp::LineTo { x, y } => s.push_str(&format!("{} {} l\n", fmt(*x), fmt(*y))),
            PathOp::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => s.push_str(&format!(
                "{} {} {} {} {} {} c\n",
                fmt(*x1),
                fmt(*y1),
                fmt(*x2),
                fmt(*y2),
                fmt(*x3),
                fmt(*y3)
            )),
            PathOp::Close => s.push_str("h\n"),
        }
    }
    s.push_str(paint_op(stroke.is_some(), fill.is_some()));
    s.push_str("\nQ\n");
    s
}

fn paint_op(stroke: bool, fill: bool) -> &'static str {
    match (stroke, fill) {
        (true, true) => "B",
        (false, true) => "f",
        (true, false) => "S",
        (false, false) => "n",
    }
}

/// Approximate an ellipse with four cubic Bézier curves (kappa ≈ 0.5522847498).
fn ellipse_path_ops(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<PathOp> {
    const K: f32 = 0.552_284_8;
    let kx = rx * K;
    let ky = ry * K;
    vec![
        PathOp::MoveTo { x: cx + rx, y: cy },
        PathOp::CurveTo {
            x1: cx + rx,
            y1: cy + ky,
            x2: cx + kx,
            y2: cy + ry,
            x3: cx,
            y3: cy + ry,
        },
        PathOp::CurveTo {
            x1: cx - kx,
            y1: cy + ry,
            x2: cx - rx,
            y2: cy + ky,
            x3: cx - rx,
            y3: cy,
        },
        PathOp::CurveTo {
            x1: cx - rx,
            y1: cy - ky,
            x2: cx - kx,
            y2: cy - ry,
            x3: cx,
            y3: cy - ry,
        },
        PathOp::CurveTo {
            x1: cx + kx,
            y1: cy - ry,
            x2: cx + rx,
            y2: cy - ky,
            x3: cx + rx,
            y3: cy,
        },
        PathOp::Close,
    ]
}

fn fmt(v: f32) -> String {
    // Trim trailing zeros for compact content streams
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn color_ops(c: &Color) -> String {
    format!("{} {} {}", fmt(c.r), fmt(c.g), fmt(c.b))
}

fn escape_pdf_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::validate_pdf_bytes;

    #[test]
    fn test_line_content_stream() {
        let canvas = VectorCanvas::new().line(10.0, 20.0, 30.0, 40.0, Color::black(), 1.5);
        let stream = canvas.to_content_stream();
        assert!(stream.contains("10 20 m"));
        assert!(stream.contains("30 40 l"));
        assert!(stream.contains("S\n"));
    }

    #[test]
    fn test_rect_fill_and_stroke() {
        let canvas = VectorCanvas::new().rect(
            0.0,
            0.0,
            100.0,
            50.0,
            Some(Color::black()),
            Some(Color::red()),
            1.0,
        );
        let stream = canvas.to_content_stream();
        assert!(stream.contains("0 0 100 50 re"));
        assert!(stream.contains("B\n"));
        assert!(stream.contains("1 0 0 rg"));
    }

    #[test]
    fn test_ellipse_uses_curves() {
        let canvas =
            VectorCanvas::new().ellipse(100.0, 100.0, 40.0, 20.0, Some(Color::blue()), None, 1.0);
        let stream = canvas.to_content_stream();
        assert!(stream.contains(" c\n"));
        assert!(stream.contains("S\n"));
    }

    #[test]
    fn test_polygon_closes() {
        let canvas = VectorCanvas::new().polygon(
            vec![(0.0, 0.0), (10.0, 0.0), (5.0, 10.0)],
            Some(Color::black()),
            None,
            1.0,
        );
        let stream = canvas.to_content_stream();
        assert!(stream.contains("h\n"));
        assert!(stream.contains("0 0 m"));
    }

    #[test]
    fn test_demo_pdf_valid() {
        let bytes = demo_canvas().to_pdf_bytes(PageLayout::portrait()).unwrap();
        let validation = validate_pdf_bytes(&bytes);
        assert!(validation.valid, "{:?}", validation.errors);
        assert!(validation.page_count >= 1);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains(" re\n") || text.contains(" re"));
        assert!(text.contains(" c\n") || text.contains(" c"));
    }

    #[test]
    fn test_paint_mode_ops() {
        assert_eq!(paint_op(true, true), "B");
        assert_eq!(paint_op(false, true), "f");
        assert_eq!(paint_op(true, false), "S");
        assert_eq!(paint_op(false, false), "n");
    }

    #[test]
    fn test_parse_svg_path_basic() {
        let ops = parse_svg_path("M10 20 L30 40 Z").unwrap();
        assert_eq!(
            ops,
            vec![
                PathOp::MoveTo { x: 10.0, y: 20.0 },
                PathOp::LineTo { x: 30.0, y: 40.0 },
                PathOp::Close,
            ]
        );
    }

    #[test]
    fn test_parse_svg_path_relative_and_hv() {
        let ops = parse_svg_path("M10,10 h20 v-5 z").unwrap();
        assert_eq!(ops[0], PathOp::MoveTo { x: 10.0, y: 10.0 });
        assert_eq!(ops[1], PathOp::LineTo { x: 30.0, y: 10.0 });
        assert_eq!(ops[2], PathOp::LineTo { x: 30.0, y: 5.0 });
        assert_eq!(ops[3], PathOp::Close);
    }

    #[test]
    fn test_parse_svg_cubic_and_quad() {
        let cubic = parse_svg_path("M0 0 C10 0 10 10 0 10").unwrap();
        assert!(matches!(cubic[1], PathOp::CurveTo { .. }));

        let quad = parse_svg_path("M0 0 Q10 10 20 0").unwrap();
        assert!(matches!(quad[1], PathOp::CurveTo { .. }));
    }

    #[test]
    fn test_parse_svg_arc_rejected() {
        assert!(parse_svg_path("M0 0 A10 10 0 0 1 20 0").is_err());
    }

    #[test]
    fn test_extract_svg_path_d() {
        let svg = r#"<svg><path fill="none" d="M1 2 L3 4"/></svg>"#;
        assert_eq!(extract_svg_path_d(svg).unwrap(), "M1 2 L3 4");
    }

    #[test]
    fn test_svg_path_to_pdf_bytes() {
        let bytes = svg_path_to_pdf_bytes(
            "M72 72 L300 72 L186 220 Z",
            PageLayout::portrait(),
            Some(Color::black()),
            Some(Color::rgb(0.9, 0.9, 1.0)),
            2.0,
        )
        .unwrap();
        let validation = validate_pdf_bytes(&bytes);
        assert!(validation.valid, "{:?}", validation.errors);
    }

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
    fn test_svg_transform_translate() {
        let m = parse_svg_transform("translate(100,50)");
        assert_eq!(m, [1.0, 0.0, 0.0, 1.0, 100.0, 50.0]);
    }

    #[test]
    fn test_svg_transform_scale() {
        let m = parse_svg_transform("scale(2,3)");
        assert_eq!(m, [2.0, 0.0, 0.0, 3.0, 0.0, 0.0]);
        // Single-arg scale duplicates.
        let m = parse_svg_transform("scale(2)");
        assert_eq!(m, [2.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
    }

    #[test]
    fn test_svg_transform_rotate_around_origin() {
        let m = parse_svg_transform("rotate(90)");
        let cos90 = 0.0f32;
        let sin90 = 1.0f32;
        assert!((m[0] - cos90).abs() < 1e-5);
        assert!((m[1] - sin90).abs() < 1e-5);
        assert!((m[2] + sin90).abs() < 1e-5);
        assert!((m[3] - cos90).abs() < 1e-5);
    }

    #[test]
    fn test_svg_transform_matrix() {
        let m = parse_svg_transform("matrix(1,2,3,4,5,6)");
        assert_eq!(m, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
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
        let svg = r##"<svg width="100" height="100"><rect width="50" height="50" fill="red"/></svg>"##;
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
                assert!(ops.len() > 4, "rounded rect should produce a path with curves");
            }
            VectorShape::Rect { .. } => panic!("rounded rect should be a Path, not Rect"),
            _ => panic!("unexpected shape"),
        }
    }

    #[test]
    fn test_svg_text_renders_as_text_shape() {
        let svg = r##"<svg width="200" height="100"><text x="50" y="50" font-size="14" fill="black">Hello</text></svg>"##;
        let canvas = parse_svg_document(svg, PageLayout::portrait()).unwrap();
        let has_text = canvas.shapes().iter().any(|s| matches!(s, VectorShape::Text { .. }));
        assert!(has_text, "should have a Text shape, not a Rect");
    }

    #[test]
    fn test_svg_text_pdf_contains_text_operators() {
        let svg = r##"<svg width="200" height="100"><text x="50" y="50" font-size="14" fill="black">Hello</text></svg>"##;
        let bytes = svg_document_to_pdf_bytes(svg, PageLayout::portrait()).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("BT"), "PDF should contain BT operator for text");
        assert!(text.contains("Tj"), "PDF should contain Tj operator for text");
        assert!(text.contains("/F1"), "PDF should reference /F1 font");
        assert!(text.contains("Helvetica"), "PDF should embed Helvetica font");
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
        let has_text = canvas.shapes().iter().any(|s| {
            matches!(s, VectorShape::Text { text, .. } if text.contains("CDATA text"))
        });
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
