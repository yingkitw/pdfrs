//! Native PDF page rasterization (PDF → PNG), pure Rust, no external dependencies.
//!
//! This is a **schematic** rasterizer with anti-aliased output: graphics
//! (rectangles, lines, ellipses, polygons, Bézier paths) are rendered
//! faithfully to a pixel buffer via 3× supersampling; text is rendered from
//! real glyph outlines when a TrueType font is available — embedded via
//! `/FontFile2` or a substitute system font for base-14 names — and as
//! light-gray glyph-block rectangles (positioned using standard PDF font
//! width tables, PDF 32000-1 base-14 widths) only when no font program can
//! be found.
//!
//! It produces layout-faithful PNG previews without depending on pdf.js,
//! Ghostscript, PDFium, or any external font rasterizer. The PNG encoder is
//! implemented inline (deflate IDAT chunks via [`flate2`]).
//!
//! ## Scope
//!
//! Renders the operators emitted by `pdfrs` itself plus the common PDF
//! content-stream subset used by simpler producers:
//!
//! | Operator | Meaning | Supported |
//! |---|---|---|
//! | `q` / `Q` | Save / restore graphics state | ✅ |
//! | `cm` | Concatenate matrix | ✅ |
//! | `w` | Line width | ✅ |
//! | `rg` / `RG` / `g` / `G` / `k` / `K` | Color | ✅ |
//! | `m` / `l` / `c` / `h` / `re` | Path construction | ✅ |
//! | `S` / `s` / `f` / `B` / `b` / `n` | Path painting | ✅ |
//! | `BT` / `ET` | Text object | ✅ |
//! | `Tf` | Text font/size | ✅ |
//! | `Tm` / `Td` / `TD` / `T*` | Text matrix / position | ✅ |
//! | `Tj` / `TJ` / `'` / `"` | Show text | ✅ |
//!
//! ## Limits
//!
//! - Anti-aliasing: pages render at 3× (or 2× for very large pages) and are
//!   box-downsampled, giving smooth edges on fills, strokes, and glyphs.
//! - When an embedded TrueType font is available, glyph **outlines** are
//!   rasterised via `ttf-parser` for crisp, shape-accurate text rendering.
//!   Base-14 fonts (no embedded data) fall back to gray glyph-block rectangles,
//!   unless a substitute system font (or `PDFRS_UNICODE_FONT_PATH`) is
//!   available, in which case its letterforms are used with the base-14
//!   width tables.
//! - Type 3 fonts, patterns, shadings, transparency groups, clipping paths,
//!   and images are not yet rendered.
//!
//! [`flate2`]: https://docs.rs/flate2

mod base14;
mod fonts;
mod interpreter;
mod pdf_access;
mod png;
mod surface;

use crate::error::{PdfError, Result};
use crate::pdf::{PdfDocument, PdfObject};
use crate::search::{decompress_stream, page_content_streams};
use fonts::collect_font_metrics_with_raw;
use interpreter::render_content_stream;
use pdf_access::page_media_box;
use png::encode_png;
use surface::Surface;

/// A rasterised page.
#[derive(Debug, Clone)]
pub struct RasterPage {
    /// RGBA8 pixel buffer, top-left origin.
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl RasterPage {
    /// Encode the page as a PNG byte string.
    pub fn to_png(&self) -> Result<Vec<u8>> {
        encode_png(self.width, self.height, &self.pixels)
    }

    /// Convenience: encode and write to `path`.
    pub fn write_png(&self, path: &str) -> Result<()> {
        let png = self.to_png()?;
        std::fs::write(path, png)?;
        Ok(())
    }
}

/// Rasterise a single PDF page (0-indexed) to an RGBA pixel buffer at `dpi`.
///
/// Page size honours the page's `/MediaBox` (in PDF points; 72 pt = 1 in).
pub fn rasterize_page(pdf_bytes: &[u8], page_index: usize, dpi: u32) -> Result<RasterPage> {
    let doc = PdfDocument::load_from_bytes(pdf_bytes)?;
    let pages = crate::search::collect_pages_from_doc(&doc, Some(pdf_bytes));
    let page_id = *pages.get(page_index).ok_or_else(|| {
        PdfError::InvalidInput(format!(
            "page index {page_index} out of range ({} pages)",
            pages.len()
        ))
    })?;

    let (width_pt, height_pt) = page_media_box(pdf_bytes, &doc, page_id)?;
    let scale = (dpi as f32) / 72.0;
    const MAX_DIM_PX: u32 = 32_768;
    let width_px = ((width_pt * scale).round() as u32).clamp(1, MAX_DIM_PX);
    let height_px = ((height_pt * scale).round() as u32).clamp(1, MAX_DIM_PX);

    // Supersampling factor: render at `aa`× resolution and box-downsample for
    // anti-aliased edges. Budget-capped so huge pages fall back gracefully.
    const MAX_SS_PIXELS: u64 = 48_000_000;
    let area = width_px as u64 * height_px as u64;
    let aa = if area * 9 <= MAX_SS_PIXELS {
        3
    } else if area * 4 <= MAX_SS_PIXELS {
        2
    } else {
        1
    };

    let content_ids = page_content_streams(&doc, page_id)?;
    let font_metrics = collect_font_metrics_with_raw(&doc, pdf_bytes, page_id);

    let mut surface = Surface::new(width_px * aa, height_px * aa);
    // PDF origin is bottom-left; flip Y so we can paint top-down.
    let aa_scale = scale * aa as f32;
    surface.transform = [aa_scale, 0.0, 0.0, -aa_scale, 0.0, height_pt * aa_scale];

    for cid in content_ids {
        let raw = match doc.objects.get(&cid) {
            Some(PdfObject::Stream { data, .. }) => data.clone(),
            _ => continue,
        };
        let decompressed = decompress_stream(&raw);
        let text = String::from_utf8_lossy(&decompressed).into_owned();
        render_content_stream(&mut surface, &text, &font_metrics);
    }

    let (pixels, width, height) = if aa > 1 {
        downsample(&surface.pixels, surface.width, surface.height, aa)
    } else {
        (surface.pixels, width_px, height_px)
    };

    Ok(RasterPage {
        pixels,
        width,
        height,
    })
}

/// Box-filter downsample an RGBA buffer by an integer factor (`aa` × `aa`
/// pixels averaged per output pixel). `w` and `h` must be multiples of `aa`.
fn downsample(pixels: &[u8], w: u32, h: u32, aa: u32) -> (Vec<u8>, u32, u32) {
    let ow = (w / aa) as usize;
    let oh = (h / aa) as usize;
    let a = aa as usize;
    let mut out = vec![0u8; ow * oh * 4];
    let n = (a * a) as f32;
    for oy in 0..oh {
        for ox in 0..ow {
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            for sy in 0..a {
                for sx in 0..a {
                    let i = (((oy * a + sy) * w as usize) + ox * a + sx) * 4;
                    r += pixels[i] as u32;
                    g += pixels[i + 1] as u32;
                    b += pixels[i + 2] as u32;
                }
            }
            let o = (oy * ow + ox) * 4;
            out[o] = (r as f32 / n).round() as u8;
            out[o + 1] = (g as f32 / n).round() as u8;
            out[o + 2] = (b as f32 / n).round() as u8;
            out[o + 3] = 255;
        }
    }
    (out, w / aa, h / aa)
}

/// Rasterise every page in the PDF.
///
/// Returns one `RasterPage` per page in document order.
pub fn rasterize_all(pdf_bytes: &[u8], dpi: u32) -> Result<Vec<RasterPage>> {
    let doc = PdfDocument::load_from_bytes(pdf_bytes)?;
    let pages = crate::search::collect_pages_from_doc(&doc, Some(pdf_bytes));
    let mut out = Vec::with_capacity(pages.len());
    for (i, _) in pages.iter().enumerate() {
        out.push(rasterize_page(pdf_bytes, i, dpi)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements;
    use crate::pdf_generator::{PageLayout, generate_pdf_bytes};
    use crate::vector::{VectorCanvas, demo_canvas};

    #[test]
    fn rasterize_generated_text_pdf_yields_png() {
        let elements = elements::parse_markdown("# Hello\n\nBody text *with* `code`.");
        let pdf = generate_pdf_bytes(&elements, "Helvetica", 12.0, PageLayout::portrait()).unwrap();
        let page = rasterize_page(&pdf, 0, 72).unwrap();
        let png = page.to_png().unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert!(png.len() > 100);
    }

    #[test]
    fn rasterize_vector_demo_pdf_yields_png() {
        let pdf = demo_canvas().to_pdf_bytes(PageLayout::portrait()).unwrap();
        let page = rasterize_page(&pdf, 0, 96).unwrap();
        let png = page.to_png().unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]));
    }

    #[test]
    fn rasterize_all_returns_one_per_page() {
        let mut canvas = VectorCanvas::new();
        for _ in 0..3 {
            canvas = canvas.line(
                72.0,
                700.0,
                540.0,
                700.0,
                crate::pdf_generator::Color::black(),
                1.0,
            );
        }
        // Three pages via a multi-page helper (3 separate one-page PDFs merged
        // here isn't needed; we just verify a one-page document yields one raster).
        let pdf = canvas.to_pdf_bytes(PageLayout::portrait()).unwrap();
        let pages = rasterize_all(&pdf, 72).unwrap();
        assert_eq!(pages.len(), 1);
    }

    #[test]
    fn rasterize_landscape_wider_than_tall() {
        let pdf = demo_canvas().to_pdf_bytes(PageLayout::landscape()).unwrap();
        let page = rasterize_page(&pdf, 0, 72).unwrap();
        assert!(page.width > page.height);
    }

    #[test]
    fn rasterize_page_out_of_range_errors() {
        let pdf = demo_canvas().to_pdf_bytes(PageLayout::portrait()).unwrap();
        let err = rasterize_page(&pdf, 9, 72).expect_err("should fail");
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn raster_dimensions_scale_with_dpi() {
        let pdf = demo_canvas().to_pdf_bytes(PageLayout::portrait()).unwrap();
        let p72 = rasterize_page(&pdf, 0, 72).unwrap();
        let p144 = rasterize_page(&pdf, 0, 144).unwrap();
        assert_eq!(p144.width, p72.width * 2);
        assert_eq!(p144.height, p72.height * 2);
    }

    #[test]
    fn raster_round_trips_through_png_decode() {
        // PNG signature + IHDR + IDAT + IEND structure should be present.
        let pdf = demo_canvas().to_pdf_bytes(PageLayout::portrait()).unwrap();
        let page = rasterize_page(&pdf, 0, 72).unwrap();
        let png = page.to_png().unwrap();
        let s = String::from_utf8_lossy(&png);
        assert!(s.contains("IHDR"));
        assert!(s.contains("IDAT"));
        assert!(s.contains("IEND"));
    }

    #[test]
    fn rasterize_unicode_text_with_embedded_font() {
        // Generate a PDF with Unicode text (forces embedded TTF font).
        let elements = elements::parse_markdown("# Hello αβγ\n\nUnicode text: 中文");
        let pdf = generate_pdf_bytes(&elements, "Helvetica", 12.0, PageLayout::portrait()).unwrap();
        let page = rasterize_page(&pdf, 0, 150).unwrap();
        let png = page.to_png().unwrap();
        // PNG should be valid and non-trivial in size.
        assert!(png.len() > 500);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        // The page should have non-white pixels (text was rendered).
        let non_white = page
            .pixels
            .chunks(4)
            .filter(|px| px[0] < 250 || px[1] < 250 || px[2] < 250)
            .count();
        assert!(
            non_white > 100,
            "expected rendered text pixels, got {non_white}"
        );
    }

    #[test]
    fn raster_supersampling_antialiases_fill_edges() {
        // A black rect whose left edge lands mid-pixel (x = 10.37 at 72 dpi)
        // must produce intermediate gray coverage on the boundary column.
        let canvas = VectorCanvas::new().rect(
            10.37,
            300.0,
            100.0,
            100.0,
            None,
            Some(crate::pdf_generator::Color::black()),
            1.0,
        );
        let pdf = canvas.to_pdf_bytes(PageLayout::portrait()).unwrap();
        let page = rasterize_page(&pdf, 0, 72).unwrap();
        let boundary: Vec<u8> = (400..500)
            .map(|y| page.pixels[((y * page.width as usize) + 10) * 4])
            .collect();
        let intermediate = boundary.iter().filter(|&&v| v > 30 && v < 220).count();
        assert!(
            intermediate >= 30,
            "expected anti-aliased edge pixels on rect boundary, got {intermediate}"
        );
    }

    #[test]
    fn base14_text_renders_letterforms_when_substitute_font_available() {
        // Skipped when no substitute font can be found (CI containers without
        // fonts and no PDFRS_UNICODE_FONT_PATH): the gray-rect fallback applies.
        let Some(_font) = fonts::substitute_font_bytes() else {
            return;
        };

        let elements = elements::parse_markdown("# Test");
        let pdf = generate_pdf_bytes(&elements, "Helvetica", 12.0, PageLayout::portrait()).unwrap();
        let page = rasterize_page(&pdf, 0, 150).unwrap();

        // Letterform outlines are solid black fills: expect very dark pixels
        // (the gray-rect fallback only reaches ~140 on white).
        let darkest = page.pixels.chunks(4).map(|px| px[0]).min().unwrap_or(255);
        assert!(
            darkest < 80,
            "expected solid black glyph pixels from substitute font, darkest={darkest}"
        );
    }
}
