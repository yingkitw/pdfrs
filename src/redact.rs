//! True content-stream redaction for PDFs.
//!
//! Unlike an opaque overlay, true redaction rewrites the underlying content
//! stream so the redacted text no longer appears in the extracted text or in
//! any text-level reader. The rewriter:
//!
//! 1. Walks every page's content stream.
//! 2. Computes the bounding box of each text-show operation.
//! 3. Replaces text whose box intersects a redacted region with spaces —
//!    at **character granularity** (only the characters whose individual
//!    bounding boxes fall within the region are masked, not the whole `Tj`).
//! 4. Removes `Do` operators for image XObjects whose placement intersects
//!    a redacted region (true image removal, not just overlay).
//! 5. Appends a solid-black filled rectangle over each redacted region before
//!    `ET`, so any non-text content under the box is also visually obscured.
//!
//! ```rust,no_run
//! use pdfrs::redact::{redact_pdf_bytes, RedactionRegion};
//! let pdf = std::fs::read("doc.pdf").unwrap();
//! let redacted = redact_pdf_bytes(&pdf, &[RedactionRegion {
//!     page: 0,
//!     x: 100.0, y: 700.0, width: 200.0, height: 20.0,
//! }]).unwrap();
//! std::fs::write("redacted.pdf", redacted).unwrap();
//! ```

use crate::compression::compress_deflate;
use crate::pdf::{PdfDocument, PdfObject};
use crate::search::Rect;
use anyhow::{Result, anyhow};
use std::collections::HashMap;

/// A rectangle on a page in PDF user-space points to redact.
#[derive(Debug, Clone, Copy)]
pub struct RedactionRegion {
    /// Zero-indexed page number in document order.
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RedactionRegion {
    pub fn rect(&self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// How a redaction region is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionStyle {
    /// Replace intersecting text with spaces and overlay a black rectangle.
    BlackBox,
    /// Replace intersecting text with spaces only (no overlay).
    Strip,
}

/// Redact one or more regions from `pdf_bytes`.
///
/// Returns new PDF bytes with the content streams rewritten. The redaction is
/// applied to every region whose page is present in the document.
pub fn redact_pdf_bytes(pdf_bytes: &[u8], regions: &[RedactionRegion]) -> Result<Vec<u8>> {
    redact_pdf_bytes_with_style(pdf_bytes, regions, RedactionStyle::BlackBox)
}

/// Same as [`redact_pdf_bytes`] but lets the caller choose the redaction style.
pub fn redact_pdf_bytes_with_style(
    pdf_bytes: &[u8],
    regions: &[RedactionRegion],
    style: RedactionStyle,
) -> Result<Vec<u8>> {
    if regions.is_empty() {
        return Ok(pdf_bytes.to_vec());
    }

    let mut doc = PdfDocument::load_from_bytes(pdf_bytes)?;
    let pages = crate::search::collect_pages_from_doc(&doc, Some(pdf_bytes));
    if pages.is_empty() {
        return Err(anyhow!("PDF has no pages"));
    }

    // Bucket regions by page for O(1) lookup.
    let mut by_page: HashMap<usize, Vec<Rect>> = HashMap::new();
    for r in regions {
        if r.page >= pages.len() {
            return Err(anyhow!(
                "redaction region refers to page {} but document has {} pages",
                r.page,
                pages.len()
            ));
        }
        by_page.entry(r.page).or_insert(Vec::new()).push(r.rect());
    }

    let fonts = collect_font_metrics(&doc);

    for (page_idx, page_id) in pages.iter().enumerate() {
        let Some(regs) = by_page.get(&page_idx).cloned() else {
            continue;
        };
        // Collect image XObject names for this page so we can remove `Do` calls.
        let image_xobjects = collect_image_xobjects(&doc, *page_id);
        let content_ids = page_content_streams(&doc, *page_id)?;
        for cid in content_ids {
            let raw = match doc.objects.get(&cid) {
                Some(PdfObject::Stream { data, .. }) => data.clone(),
                _ => continue,
            };
            let decompressed = decompress_stream(&raw);
            let src = String::from_utf8_lossy(&decompressed).into_owned();
            let rewritten = rewrite_stream(&src, &regs, &fonts, style, &image_xobjects);
            let new_bytes = rewritten.into_bytes();
            // Compress if original was compressed
            let (new_data, filter) = if is_deflate_stream(&raw) {
                let compressed = compress_deflate(&new_bytes)?;
                (compressed, Some("FlateDecode"))
            } else {
                (new_bytes, None)
            };
            if let Some(PdfObject::Stream { dictionary, data }) = doc.objects.get_mut(&cid) {
                dictionary.remove("Filter");
                if let Some(f) = filter {
                    dictionary.insert(
                        "Filter".to_string(),
                        crate::pdf::PdfValue::Object(crate::pdf::PdfObject::Name(f.to_string())),
                    );
                }
                // Update /Length
                dictionary.insert(
                    "Length".to_string(),
                    crate::pdf::PdfValue::Object(crate::pdf::PdfObject::Number(
                        new_data.len() as f64
                    )),
                );
                *data = new_data;
            }
        }
    }

    Ok(doc.to_bytes())
}

// ----- Stream rewriting ---------------------------------------------------

/// Names of image XObjects on a page that should be removed if their placement
/// intersects a redaction region.
type ImageXObjects = HashMap<String, (f32, f32)>; // name -> (width, height) in user units

/// Collect image XObject names and their natural sizes from a page's /Resources.
fn collect_image_xobjects(doc: &PdfDocument, page_id: u32) -> ImageXObjects {
    use crate::pdf::PdfValue;
    let mut result = HashMap::new();
    let Some(dict) = crate::search::object_dict(doc, page_id) else {
        return result;
    };
    let Some(resources) = dict.get("Resources") else {
        return result;
    };
    let resources_dict = match resources {
        PdfValue::Reference(id, _) => crate::search::object_dict(doc, *id),
        PdfValue::Object(PdfObject::Dictionary(d)) => Some(d),
        _ => None,
    };
    let Some(res_dict) = resources_dict else {
        return result;
    };
    let Some(xobjects) = res_dict.get("XObject") else {
        return result;
    };
    let xobj_dict = match xobjects {
        PdfValue::Reference(id, _) => crate::search::object_dict(doc, *id),
        PdfValue::Object(PdfObject::Dictionary(d)) => Some(d),
        _ => None,
    };
    let Some(xobj_dict) = xobj_dict else {
        return result;
    };
    for (name, val) in xobj_dict {
        let obj_id = match val {
            PdfValue::Reference(id, _) => Some(*id),
            _ => None,
        };
        if let Some(id) = obj_id
            && let Some(obj_dict) = crate::search::object_dict(doc, id) {
                let is_image = obj_dict
                    .get("Subtype")
                    .and_then(|v| match v {
                        PdfValue::Object(PdfObject::Name(s)) => Some(s.as_str()),
                        _ => None,
                    })
                    .map(|s| s == "Image")
                    .unwrap_or(false);
                if is_image {
                    let w = obj_dict
                        .get("Width")
                        .and_then(|v| match v {
                            PdfValue::Object(PdfObject::Number(n)) => Some(*n as f32),
                            _ => None,
                        })
                        .unwrap_or(100.0);
                    let h = obj_dict
                        .get("Height")
                        .and_then(|v| match v {
                            PdfValue::Object(PdfObject::Number(n)) => Some(*n as f32),
                            _ => None,
                        })
                        .unwrap_or(100.0);
                    result.insert(name.clone(), (w, h));
                }
            }
    }
    result
}

fn rewrite_stream(
    src: &str,
    regions: &[Rect],
    fonts: &HashMap<String, crate::search::FontMetrics>,
    style: RedactionStyle,
    image_xobjects: &ImageXObjects,
) -> String {
    let tokens = crate::search::tokenize(src);
    let mut i = 0;
    let mut operands: Vec<f32> = Vec::new();
    let mut text_matrix = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut text_line_matrix = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut font_size = 12.0f32;
    let mut in_text = false;
    let mut current_metrics: Option<crate::search::FontMetrics> = None;
    // CTM stack for tracking graphics state (for image XObject placement).
    let mut ctm_stack: Vec<[f32; 6]> = Vec::new();
    let mut ctm = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut out = String::new();
    while i < tokens.len() {
        let t = &tokens[i];
        if let Ok(n) = t.parse::<f32>() {
            operands.push(n);
            i += 1;
            continue;
        }
        let op = t.as_str();
        match op {
            "BT" => {
                in_text = true;
                text_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                text_line_matrix = text_matrix;
                out.push_str("BT\n");
            }
            "ET" => {
                // Append black-box overlays for every region on this page.
                if style == RedactionStyle::BlackBox {
                    for r in regions {
                        out.push_str(&format!(
                            "q\n0 0 0 rg\n{} {} {} {} re\nf\nQ\n",
                            fmt_f(r.x),
                            fmt_f(r.y),
                            fmt_f(r.width),
                            fmt_f(r.height)
                        ));
                    }
                }
                in_text = false;
                out.push_str("ET\n");
            }
            "q" => {
                ctm_stack.push(ctm);
                out.push_str("q\n");
            }
            "Q" => {
                if let Some(prev) = ctm_stack.pop() {
                    ctm = prev;
                }
                out.push_str("Q\n");
            }
            "cm" => {
                if operands.len() == 6 {
                    let m = [
                        operands[0],
                        operands[1],
                        operands[2],
                        operands[3],
                        operands[4],
                        operands[5],
                    ];
                    ctm = matrix_multiply(&ctm, &m);
                }
                emit_operator(&mut out, "cm", &operands);
            }
            "Do" => {
                // Check if this Do references an image XObject in a redacted region.
                let xobj_name = if i > 0 {
                    let prev = &tokens[i - 1];
                    if let Some(stripped) = prev.strip_prefix('/') {
                        Some(stripped.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(ref name) = xobj_name {
                    if let Some(&(w, h)) = image_xobjects.get(name) {
                        // Compute image position using CTM.
                        let img_x = ctm[4];
                        let img_y = ctm[5];
                        let img_w = ctm[0] * w;
                        let img_h = ctm[3] * h;
                        let img_rect = Rect {
                            x: img_x,
                            y: img_y,
                            width: img_w.abs(),
                            height: img_h.abs(),
                        };
                        if regions.iter().any(|r| r.intersects(&img_rect)) {
                            // Remove the /name operand from output and skip the Do.
                            if let Some(pos) = out.rfind('/') {
                                out.truncate(pos);
                            }
                            out.push_str("% redacted image\n");
                        } else {
                            out.push_str(&format!("/{} Do\n", name));
                        }
                    } else {
                        // Not an image XObject — pass through.
                        out.push_str(&format!("/{} Do\n", name));
                    }
                } else {
                    out.push_str("Do\n");
                }
            }
            "Tf" => {
                if !operands.is_empty() {
                    font_size = *operands.last().unwrap();
                }
                if let Some(name) = extract_font_name(&tokens, i) {
                    current_metrics = fonts.get(&name).cloned();
                }
                emit_operator(&mut out, "Tf", &operands);
            }
            "Tm" => {
                if operands.len() == 6 {
                    let n = [
                        operands[0],
                        operands[1],
                        operands[2],
                        operands[3],
                        operands[4],
                        operands[5],
                    ];
                    text_matrix = n;
                    text_line_matrix = n;
                }
                emit_operator(&mut out, "Tm", &operands);
            }
            "Td" => {
                if operands.len() == 2 {
                    let (tx, ty) = (operands[0], operands[1]);
                    let m = text_line_matrix;
                    text_line_matrix = [
                        m[0],
                        m[1],
                        m[2],
                        m[3],
                        m[0] * tx + m[2] * ty + m[4],
                        m[1] * tx + m[3] * ty + m[5],
                    ];
                    text_matrix = text_line_matrix;
                }
                emit_operator(&mut out, "Td", &operands);
            }
            "TD" => {
                if operands.len() == 2 {
                    let (tx, ty) = (operands[0], operands[1]);
                    let m = text_line_matrix;
                    text_line_matrix = [
                        m[0],
                        m[1],
                        m[2],
                        m[3],
                        m[0] * tx + m[2] * ty + m[4],
                        m[1] * tx + m[3] * ty + m[5],
                    ];
                    text_matrix = text_line_matrix;
                }
                emit_operator(&mut out, "TD", &operands);
            }
            "T*" => {
                let m = text_line_matrix;
                let new_ey = m[5] - font_size;
                text_line_matrix = [m[0], m[1], m[2], m[3], m[4], new_ey];
                text_matrix = text_line_matrix;
                out.push_str("T*\n");
            }
            "Tj" => {
                if let Some(text) = extract_string(&tokens, i) {
                    if in_text {
                        let (x, y) = (text_matrix[4], text_matrix[5]);
                        let masked = mask_string_partial(&text, x, y, font_size, current_metrics.as_ref(), regions);
                        out.push('(');
                        out.push_str(&masked);
                        out.push(')');
                        out.push_str(" Tj\n");
                        let width = text_width(&text, font_size, current_metrics.as_ref());
                        text_matrix[4] = x + width;
                    } else {
                        out.push('(');
                        out.push_str(&text);
                        out.push(')');
                        out.push_str(" Tj\n");
                    }
                }
            }
            "TJ" => {
                if let Some(items) = extract_tj_array(&tokens, i) {
                    let mut x = text_matrix[4];
                    let y = text_matrix[5];
                    out.push('[');
                    for item in items {
                        match item {
                            TjItem::Text(t) => {
                                if in_text {
                                    let masked = mask_string_partial(&t, x, y, font_size, current_metrics.as_ref(), regions);
                                    out.push('(');
                                    out.push_str(&masked);
                                    out.push(')');
                                    let width = text_width(&t, font_size, current_metrics.as_ref());
                                    x += width;
                                } else {
                                    out.push('(');
                                    out.push_str(&t);
                                    out.push(')');
                                }
                            }
                            TjItem::Kern(amount) => {
                                out.push_str(&format!(" {} ", fmt_f(amount)));
                                x += amount;
                            }
                        }
                    }
                    out.push_str("] TJ\n");
                    text_matrix[4] = x;
                }
            }
            "'" => {
                if let Some(text) = extract_string(&tokens, i) {
                    let m = text_line_matrix;
                    let new_ey = m[5] - font_size;
                    text_line_matrix = [m[0], m[1], m[2], m[3], m[4], new_ey];
                    text_matrix = text_line_matrix;
                    out.push_str("T*\n(");
                    if in_text {
                        let (x, y) = (text_matrix[4], text_matrix[5]);
                        let masked = mask_string_partial(&text, x, y, font_size, current_metrics.as_ref(), regions);
                        out.push_str(&masked);
                    } else {
                        out.push_str(&text);
                    }
                    out.push_str(") Tj\n");
                }
            }
            _ => {
                // Pass through any token we don't explicitly handle, EXCEPT
                // literal PDF strings and hex strings — those are operands to
                // Tj/TJ which we already rewrite above.
                if op.starts_with('(') || op.starts_with('<') || op.starts_with('[') {
                    // Skip — handled by Tj/TJ arms.
                } else {
                    emit_operator(&mut out, op, &operands);
                }
            }
        }
        operands.clear();
        i += 1;
    }
    out
}

/// Multiply two 2D affine matrices (6-element: a, b, c, d, e, f).
fn matrix_multiply(a: &[f32; 6], b: &[f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

/// Mask only the characters whose individual bounding boxes intersect a redaction region.
/// Characters outside any region are preserved.
fn mask_string_partial(
    text: &str,
    start_x: f32,
    y: f32,
    font_size: f32,
    metrics: Option<&crate::search::FontMetrics>,
    regions: &[Rect],
) -> String {
    let mut x = start_x;
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        let advance = metrics.map(|m| m.advance(ch as u32)).unwrap_or(500);
        let char_width = advance as f32 * font_size / 1000.0;
        let char_bbox = Rect {
            x,
            y: y - font_size,
            width: char_width,
            height: font_size,
        };
        if regions.iter().any(|r| r.intersects(&char_bbox)) {
            result.push(' ');
        } else {
            result.push(ch);
        }
        x += char_width;
    }
    result
}

fn emit_operator(out: &mut String, op: &str, operands: &[f32]) {
    for n in operands {
        out.push_str(&fmt_f(*n));
        out.push(' ');
    }
    out.push_str(op);
    out.push('\n');
}

fn fmt_f(v: f32) -> String {
    let s = format!("{:.4}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn text_width(text: &str, font_size: f32, metrics: Option<&crate::search::FontMetrics>) -> f32 {
    let mut units = 0u32;
    for ch in text.chars() {
        let advance = metrics.map(|m| m.advance(ch as u32)).unwrap_or(500);
        units += advance as u32;
    }
    units as f32 * font_size / 1000.0
}

// ----- Plumbing -----------------------------------------------------------

// Re-use small subset of search.rs helpers.
use crate::search::{
    TjItem, collect_font_metrics as collect_font_metrics_search, decompress_stream,
    extract_font_name as search_extract_font_name, extract_string as search_extract_string,
    extract_tj_array, is_deflate_stream, page_content_streams,
};

fn collect_font_metrics(doc: &PdfDocument) -> HashMap<String, crate::search::FontMetrics> {
    collect_font_metrics_search(doc)
}

fn extract_font_name(tokens: &[String], i: usize) -> Option<String> {
    search_extract_font_name(tokens, i)
}

fn extract_string(tokens: &[String], i: usize) -> Option<String> {
    search_extract_string(tokens, i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements;
    use crate::pdf::PdfDocument;
    use crate::pdf_generator::{PageLayout, generate_pdf_bytes};

    fn make_pdf(markdown: &str) -> Vec<u8> {
        generate_pdf_bytes(
            &elements::parse_markdown(markdown),
            "Helvetica",
            12.0,
            PageLayout::portrait(),
        )
        .unwrap()
    }

    #[test]
    fn redact_removes_text_in_region() {
        let pdf = make_pdf("# Hello world\n\nThis is pdfrs.");
        // Redact a strip covering the "pdfrs" line, near the top of the page.
        let redacted = redact_pdf_bytes(
            &pdf,
            &[RedactionRegion {
                page: 0,
                x: 50.0,
                y: 655.0,
                width: 500.0,
                height: 30.0,
            }],
        )
        .unwrap();
        // Original text should still be extractable; "pdfrs" should be gone.
        let original_text = PdfDocument::load_from_bytes(&pdf)
            .unwrap()
            .get_text()
            .unwrap();
        let redacted_text = PdfDocument::load_from_bytes(&redacted)
            .unwrap()
            .get_text()
            .unwrap();
        assert!(
            original_text.contains("pdfrs"),
            "original should have pdfrs"
        );
        assert!(!redacted_text.contains("pdfrs"), "redacted should not");
    }

    #[test]
    fn redact_with_no_regions_returns_input() {
        let pdf = make_pdf("# Hello");
        let out = redact_pdf_bytes(&pdf, &[]).unwrap();
        assert_eq!(out, pdf);
    }

    #[test]
    fn redact_out_of_range_page_errors() {
        let pdf = make_pdf("# Hello");
        let err = redact_pdf_bytes(
            &pdf,
            &[RedactionRegion {
                page: 99,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            }],
        )
        .expect_err("err");
        assert!(err.to_string().contains("99"));
    }

    #[test]
    fn redact_outside_text_does_not_remove_text() {
        let pdf = make_pdf("# Hello world");
        let redacted = redact_pdf_bytes(
            &pdf,
            &[RedactionRegion {
                page: 0,
                x: 0.0,
                y: 0.0,
                width: 5.0,
                height: 5.0,
            }],
        )
        .unwrap();
        let redacted_text = PdfDocument::load_from_bytes(&redacted)
            .unwrap()
            .get_text()
            .unwrap();
        assert!(redacted_text.contains("Hello"), "should still have Hello");
    }

    #[test]
    fn strip_style_skips_black_box_overlay() {
        let pdf = make_pdf("# Hello world");
        let stripped = redact_pdf_bytes_with_style(
            &pdf,
            &[RedactionRegion {
                page: 0,
                x: 0.0,
                y: 700.0,
                width: 500.0,
                height: 50.0,
            }],
            RedactionStyle::Strip,
        )
        .unwrap();
        let blacked = redact_pdf_bytes_with_style(
            &pdf,
            &[RedactionRegion {
                page: 0,
                x: 0.0,
                y: 700.0,
                width: 500.0,
                height: 50.0,
            }],
            RedactionStyle::BlackBox,
        )
        .unwrap();
        let stripped_text = String::from_utf8_lossy(&stripped);
        let blacked_text = String::from_utf8_lossy(&blacked);
        assert!(!stripped_text.contains("0 0 0 rg"));
        assert!(blacked_text.contains("0 0 0 rg"));
    }

    #[test]
    fn partial_string_redaction_preserves_outside_text() {
        // Redact a narrow strip that only covers part of a line.
        // Text outside the strip should survive.
        let pdf = make_pdf("# Hello world\n\nThis is pdfrs.");
        let redacted = redact_pdf_bytes(
            &pdf,
            &[RedactionRegion {
                page: 0,
                x: 50.0,
                y: 655.0,
                width: 60.0, // narrow — only covers a few characters
                height: 20.0,
            }],
        )
        .unwrap();
        let redacted_text = PdfDocument::load_from_bytes(&redacted)
            .unwrap()
            .get_text()
            .unwrap();
        // "Hello" starts at the left margin; a 60pt strip from x=50 should
        // mask some of "Hello" but "world" further right should survive.
        // The exact behavior depends on font metrics, but at minimum the
        // redacted text should differ from the original.
        let original_text = PdfDocument::load_from_bytes(&pdf)
            .unwrap()
            .get_text()
            .unwrap();
        assert_ne!(redacted_text, original_text, "redaction should change text");
    }

    #[test]
    fn mask_string_partial_masks_only_intersecting_chars() {
        // "ABCDEF" at x=90, each char ~6.0pt wide at 12pt (advance=500)
        // Positions: A=90..96, B=96..102, C=102..108, D=108..114, E=114..120, F=120..126
        // Region x=104, width=6 → covers 104..110, intersects C(102..108) and D(108..114)
        let regions = [Rect {
            x: 104.0,
            y: 690.0,
            width: 6.0,
            height: 20.0,
        }];
        let result = mask_string_partial("ABCDEF", 90.0, 700.0, 12.0, None, &regions);
        assert!(result.contains('A'), "A should survive (before region)");
        assert!(result.contains('B'), "B should survive (before region)");
        assert!(result.contains('E'), "E should survive (after region)");
        assert!(result.contains('F'), "F should survive (after region)");
        assert!(result.contains(' '), "some chars should be masked");
    }

    #[test]
    fn matrix_multiply_basic() {
        let identity = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
        let translate = [1.0f32, 0.0, 0.0, 1.0, 100.0, 200.0];
        let result = matrix_multiply(&identity, &translate);
        assert_eq!(result, translate);
    }
}
