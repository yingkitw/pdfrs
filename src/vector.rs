//! Vector graphics for PDF content streams
//!
//! Provides path-based drawing primitives (lines, rectangles, ellipses, polygons,
//! and cubic Bézier paths) that compile to standard PDF operators (`m`, `l`, `c`,
//! `re`, `S`, `f`, `B`, etc.).

use crate::pdf_generator::{Color, PageLayout, PdfGenerator};
use anyhow::Result;

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
    MoveTo { x: f32, y: f32 },
    LineTo { x: f32, y: f32 },
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

    pub fn add(mut self, shape: VectorShape) -> Self {
        self.shapes.push(shape);
        self
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
        // page will be next_id, pages the one after that
        let pages_id = generator.next_id + 1;

        let page_dict = format!(
            "<< /Type /Page\n\
             /Parent {} 0 R\n\
             /MediaBox [0 0 {} {}]\n\
             /Contents {} 0 R\n\
             /Resources << >>\n\
             >>\n",
            pages_id, layout.width, layout.height, content_id
        );
        let page_id = generator.add_object(page_dict);

        let pages_dict = format!(
            "<< /Type /Pages\n/Kids [{} 0 R]\n/Count 1\n>>\n",
            page_id
        );
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
                        ))
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
                let (x3, y3) = if relative { (cx + x3, cy + y3) } else { (x3, y3) };
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

fn quad_to_cubic(
    x0: f32,
    y0: f32,
    qx: f32,
    qy: f32,
    x3: f32,
    y3: f32,
) -> (f32, f32, f32, f32) {
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
        PathOp::MoveTo {
            x: cx + rx,
            y: cy,
        },
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
        let canvas = VectorCanvas::new().ellipse(
            100.0,
            100.0,
            40.0,
            20.0,
            Some(Color::blue()),
            None,
            1.0,
        );
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
        let bytes = demo_canvas()
            .to_pdf_bytes(PageLayout::portrait())
            .unwrap();
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
}
