//! Vector graphics for PDF content streams
//!
//! Provides path-based drawing primitives (lines, rectangles, ellipses, polygons,
//! and cubic Bézier paths) that compile to standard PDF operators (`m`, `l`, `c`,
//! `re`, `S`, `f`, `B`, etc.).

macro_rules! vector_regex {
    ($name:ident, $pat:literal) => {
        fn $name() -> &'static regex::Regex {
            static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            RE.get_or_init(|| regex::Regex::new($pat).unwrap())
        }
    };
}

mod emit;
mod path;
mod svg_document;
mod transform;
mod xml;

pub use emit::{demo_canvas, extract_svg_path_d, svg_file_to_pdf, svg_path_to_pdf_bytes};
pub use path::parse_svg_path;
pub use svg_document::{parse_svg_document, svg_document_file_to_pdf, svg_document_to_pdf_bytes};
pub use transform::parse_svg_transform;

use crate::error::Result;
use crate::pdf_generator::{Color, PageLayout, PdfGenerator};

use emit::shape_to_ops;

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
        let font_id = generator
            .add_object("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n".to_string());

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
