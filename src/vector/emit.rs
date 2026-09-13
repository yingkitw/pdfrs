//! PDF content-stream emission and SVG file conversion entry points.

use super::{PathOp, VectorCanvas, VectorShape};
use crate::error::{PdfError, Result};
use crate::pdf_generator::{Color, PageLayout};

vector_regex!(
    re_svg_path_attr,
    r#"(?i)<path\b[^>]*\bd\s*=\s*["']([^"']+)["']"#
);
vector_regex!(re_svg_d_attr, r#"(?i)\bd\s*=\s*["']([^"']+)["']"#);

/// Extract the first `d="..."` path from a simple SVG document string.
pub fn extract_svg_path_d(svg: &str) -> Result<String> {
    // Prefer path elements; fall back to any d="..."
    let re = re_svg_path_attr();
    if let Some(caps) = re.captures(svg) {
        return Ok(caps[1].to_string());
    }
    let re_any = re_svg_d_attr();
    if let Some(caps) = re_any.captures(svg) {
        return Ok(caps[1].to_string());
    }
    Err(PdfError::Svg("No SVG path d= attribute found".into()))
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

pub(super) fn shape_to_ops(shape: &VectorShape) -> String {
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
