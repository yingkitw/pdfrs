//! Full SVG document model and renderer: groups, transforms, basic shapes,
//! text, `<use>`/`<defs>`, and styling attributes.

use super::VectorCanvas;
use super::xml::parse_svg_xml;
use crate::error::Result;
use crate::pdf_generator::{Color, PageLayout};
use std::collections::HashMap;

mod context;
mod shapes;
mod style;

use context::{RenderCtx, SvgPaint};
use shapes::{apply_svg_viewbox, collect_defs, render_element};

pub(crate) use context::{SvgElement, SvgNode};

// ----- Full SVG document support ------------------------------------------

/// Parse a full SVG document into a [`VectorCanvas`], honoring groups,
/// transforms, basic shapes, text, and styling attributes.
///
/// Supported elements:
/// - `<svg>` root with `viewBox` and `width`/`height`
/// - `<g>` with `transform="translate|rotate|scale|matrix(...)"`
/// - `<rect>`, `<circle>`, `<ellipse>`, `<line>`, `<polyline>`, `<polygon>`
/// - `<path d="...">` (delegates to [`crate::vector::parse_svg_path`])
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

#[cfg(test)]
mod tests {
    use super::context::SvgPaint;
    use super::style::parse_paint;
    use super::*;
    use crate::pdf::validate_pdf_bytes;
    use crate::vector::VectorShape;

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
