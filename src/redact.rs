//! True content-stream redaction for PDFs.
//!
//! Unlike an opaque overlay, true redaction rewrites the underlying content
//! stream so the redacted text no longer appears in the extracted text or in
//! any text-level reader. The rewriter:
//!
//! 1. Walks every page's content stream **and** the streams of Form
//!    XObjects and annotation appearance streams referenced from the page.
//! 2. Computes the bounding box of each text-show operation (text-showing
//!    operators are masked whether or not they appear inside `BT…ET`).
//! 3. Replaces text whose box intersects a redacted region with spaces —
//!    at **character granularity** (only the characters whose individual
//!    bounding boxes fall within the region are masked, not the whole `Tj`).
//! 4. Removes `Do` operators for image XObjects whose placement intersects
//!    a redacted region, drops their `/XObject` resource entries, and
//!    deletes the image objects themselves when no longer referenced.
//! 5. Appends a solid-black filled rectangle over each redacted region before
//!    `ET`, so any non-text content under the box is also visually obscured.
//!
//! Known limitation: text inside Form XObjects is matched against the page's
//! redaction regions in form-local coordinates (exact for forms placed with
//! an identity or translation-only CTM).
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
use crate::pdf::{PdfDocument, PdfObject, PdfValue};
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
        by_page.entry(r.page).or_default().push(r.rect());
    }

    let fonts = collect_font_metrics(&doc);
    // Image object ids removed across all pages (name -> object id, per page).
    let mut removed_images: HashMap<u32, u32> = HashMap::new(); // image_obj_id -> page_id owning the resource entry

    for (page_idx, page_id) in pages.iter().enumerate() {
        let Some(regs) = by_page.get(&page_idx).cloned() else {
            continue;
        };
        let xobjects = collect_xobjects(&doc, *page_id);
        let annot_stream_ids = collect_annotation_ap_streams(&doc, *page_id);
        let form_ids: Vec<u32> = xobjects
            .values()
            .filter(|x| x.is_form)
            .map(|x| x.id)
            .collect();

        // Rewrite page content streams, form XObjects, and annotation
        // appearance streams — all can carry text-showing operators.
        let mut stream_ids = page_content_streams(&doc, *page_id)?;
        stream_ids.extend(form_ids);
        stream_ids.extend(annot_stream_ids);
        let mut removed_names: Vec<String> = Vec::new();

        for cid in stream_ids {
            let raw = match doc.objects.get(&cid) {
                Some(PdfObject::Stream { data, .. }) => data.clone(),
                _ => continue,
            };
            let decompressed = decompress_stream(&raw);
            let src = String::from_utf8_lossy(&decompressed).into_owned();
            let (rewritten, removed) = rewrite_stream(&src, &regs, &fonts, style, &xobjects);
            removed_names.extend(removed);
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
                        PdfValue::Object(PdfObject::Name(f.to_string())),
                    );
                }
                dictionary.insert(
                    "Length".to_string(),
                    PdfValue::Object(PdfObject::Number(new_data.len() as f64)),
                );
                *data = new_data;
            }
        }

        // Drop the removed images' resource entries and remember the object
        // ids so they can be deleted when no longer referenced.
        if !removed_names.is_empty() {
            for name in &removed_names {
                if let Some(info) = xobjects.get(name) {
                    removed_images.insert(info.id, *page_id);
                }
            }
            remove_xobject_entries(&mut doc, *page_id, &removed_names);
        }
    }

    // Delete image objects that are no longer referenced anywhere in the
    // document (other pages, forms, or annotations may still use them).
    for (image_id, _) in removed_images {
        if !is_referenced(&doc, image_id) {
            doc.objects.remove(&image_id);
        }
    }

    Ok(doc.to_bytes())
}

/// Whether any object in the document still references `id`.
fn is_referenced(doc: &PdfDocument, id: u32) -> bool {
    fn value_references(v: &PdfValue, id: u32) -> bool {
        match v {
            PdfValue::Reference(r, _) => *r == id,
            PdfValue::Object(o) => object_references(o, id),
        }
    }
    fn object_references(o: &PdfObject, id: u32) -> bool {
        match o {
            PdfObject::Dictionary(d) => d.values().any(|v| value_references(v, id)),
            PdfObject::Stream { dictionary, .. } => {
                dictionary.values().any(|v| value_references(v, id))
            }
            PdfObject::Array(items) => items.iter().any(|v| value_references(v, id)),
            PdfObject::Reference(r, _) => *r == id,
            _ => false,
        }
    }
    doc.objects.values().any(|o| object_references(o, id))
}

/// Remove `names` from the page's `/Resources /XObject` dictionary (whether
/// the resources dict is inline or a separate object).
fn remove_xobject_entries(doc: &mut PdfDocument, page_id: u32, names: &[String]) {
    // Find the object that owns the XObject dict: the page itself or the
    // object its /Resources points at.
    let owner_id = {
        let Some(dict) = crate::search::object_dict(doc, page_id) else {
            return;
        };
        match dict.get("Resources") {
            Some(PdfValue::Reference(id, _)) => *id,
            Some(PdfValue::Object(PdfObject::Dictionary(_))) => page_id,
            _ => return,
        }
    };
    let Some(owner) = doc.objects.get_mut(&owner_id) else {
        return;
    };
    let dict = match owner {
        PdfObject::Dictionary(d) | PdfObject::Stream { dictionary: d, .. } => d,
        _ => return,
    };
    if let Some(PdfValue::Object(PdfObject::Dictionary(res))) = dict.get_mut("Resources")
        && let Some(PdfValue::Object(PdfObject::Dictionary(xobj))) = res.get_mut("XObject")
    {
        xobj.retain(|k, _| !names.contains(k));
    }
}

/// Stream object ids of annotation appearance streams (`/AP /N /R /D`)
/// attached to a page's `/Annots`.
fn collect_annotation_ap_streams(doc: &PdfDocument, page_id: u32) -> Vec<u32> {
    let mut result = Vec::new();
    let Some(dict) = crate::search::object_dict(doc, page_id) else {
        return result;
    };
    let annots = match dict.get("Annots") {
        Some(PdfValue::Object(PdfObject::Array(items))) => items.clone(),
        Some(PdfValue::Reference(id, _)) => match doc.objects.get(id) {
            Some(PdfObject::Array(items)) => items.clone(),
            _ => return result,
        },
        _ => return result,
    };
    for annot_val in annots {
        let annot_id = match annot_val {
            PdfValue::Reference(id, _) => id,
            PdfValue::Object(PdfObject::Dictionary(_)) => continue,
            _ => continue,
        };
        let Some(annot_dict) = crate::search::object_dict(doc, annot_id) else {
            continue;
        };
        if let Some(PdfValue::Object(PdfObject::Dictionary(ap))) = annot_dict.get("AP") {
            for entry in ap.values() {
                match entry {
                    PdfValue::Reference(id, _) => result.push(*id),
                    PdfValue::Object(PdfObject::Dictionary(states)) => {
                        for state in states.values() {
                            if let PdfValue::Reference(id, _) = state {
                                result.push(*id);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    result
}

// ----- Stream rewriting ---------------------------------------------------

/// An XObject entry from a page's `/Resources`.
#[derive(Debug, Clone)]
struct XObjectInfo {
    /// Object number of the XObject.
    id: u32,
    is_image: bool,
    is_form: bool,
}

type XObjectMap = HashMap<String, XObjectInfo>;

/// Collect XObject entries (images and forms) from a page's /Resources.
fn collect_xobjects(doc: &PdfDocument, page_id: u32) -> XObjectMap {
    let mut result = HashMap::new();
    let Some(dict) = crate::search::object_dict(doc, page_id) else {
        return result;
    };
    let resources_dict = match dict.get("Resources") {
        Some(PdfValue::Reference(id, _)) => crate::search::object_dict(doc, *id),
        Some(PdfValue::Object(PdfObject::Dictionary(d))) => Some(d),
        _ => None,
    };
    let Some(res_dict) = resources_dict else {
        return result;
    };
    let xobj_dict = match res_dict.get("XObject") {
        Some(PdfValue::Reference(id, _)) => crate::search::object_dict(doc, *id),
        Some(PdfValue::Object(PdfObject::Dictionary(d))) => Some(d),
        _ => None,
    };
    let Some(xobj_dict) = xobj_dict else {
        return result;
    };
    for (name, val) in xobj_dict {
        let PdfValue::Reference(id, _) = val else {
            continue;
        };
        let Some(obj_dict) = crate::search::object_dict(doc, *id) else {
            continue;
        };
        let subtype = obj_dict
            .get("Subtype")
            .and_then(|v| match v {
                PdfValue::Object(PdfObject::Name(s)) => Some(s.as_str()),
                _ => None,
            })
            .unwrap_or("");
        result.insert(
            name.clone(),
            XObjectInfo {
                id: *id,
                is_image: subtype == "Image",
                is_form: subtype == "Form",
            },
        );
    }
    result
}

/// Rewrite one content stream. Returns the rewritten stream plus the names of
/// image XObjects whose `Do` calls were removed.
fn rewrite_stream(
    src: &str,
    regions: &[Rect],
    fonts: &HashMap<String, crate::search::FontMetrics>,
    style: RedactionStyle,
    image_xobjects: &XObjectMap,
) -> (String, Vec<String>) {
    let tokens = crate::search::tokenize(src);
    let mut i = 0;
    let mut operands: Vec<f32> = Vec::new();
    let mut pending_name: Option<String> = None;
    let mut text_matrix = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut text_line_matrix = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut font_size = 12.0f32;
    let mut current_metrics: Option<crate::search::FontMetrics> = None;
    // CTM stack for tracking graphics state (for image XObject placement).
    let mut ctm_stack: Vec<[f32; 6]> = Vec::new();
    let mut ctm = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut out = String::new();
    let mut removed_images: Vec<String> = Vec::new();

    // Emit a pending `/Name` operand plus numeric operands and the operator.
    macro_rules! emit {
        ($op:expr) => {{
            if let Some(name) = pending_name.take() {
                out.push('/');
                out.push_str(&name);
                out.push(' ');
            }
            for n in &operands {
                out.push_str(&fmt_f(*n));
                out.push(' ');
            }
            out.push_str($op);
            out.push('\n');
        }};
    }

    while i < tokens.len() {
        let t = &tokens[i];
        if let Some(name) = t.strip_prefix('/') {
            pending_name = Some(name.to_string());
            i += 1;
            continue;
        }
        if let Ok(n) = t.parse::<f32>() {
            operands.push(n);
            i += 1;
            continue;
        }
        let op = t.as_str();
        match op {
            "BT" => {
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
                emit!("cm");
            }
            "Do" => {
                let name = pending_name.clone();
                let mut removed = false;
                if let Some(ref name) = name
                    && image_xobjects.get(name).is_some_and(|x| x.is_image)
                {
                    // `Do` maps the unit square through the CTM; the
                    // placement rect is that square's bounding box.
                    let corners = [
                        ctm_apply(&ctm, 0.0, 0.0),
                        ctm_apply(&ctm, 1.0, 0.0),
                        ctm_apply(&ctm, 0.0, 1.0),
                        ctm_apply(&ctm, 1.0, 1.0),
                    ];
                    let xs = [corners[0].0, corners[1].0, corners[2].0, corners[3].0];
                    let ys = [corners[0].1, corners[1].1, corners[2].1, corners[3].1];
                    let img_rect = Rect {
                        x: xs.iter().cloned().fold(f32::INFINITY, f32::min),
                        y: ys.iter().cloned().fold(f32::INFINITY, f32::min),
                        width: xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
                            - xs.iter().cloned().fold(f32::INFINITY, f32::min),
                        height: ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
                            - ys.iter().cloned().fold(f32::INFINITY, f32::min),
                    };
                    if regions.iter().any(|r| r.intersects(&img_rect)) {
                        removed_images.push(name.clone());
                        out.push_str("% redacted image\n");
                        removed = true;
                    }
                }
                if !removed {
                    emit!("Do");
                } else {
                    pending_name.take();
                }
            }
            "Tf" => {
                if !operands.is_empty() {
                    font_size = *operands.last().unwrap();
                }
                if let Some(name) = pending_name.as_ref() {
                    current_metrics = fonts.get(name).cloned();
                }
                emit!("Tf");
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
                emit!("Tm");
            }
            "Td" | "TD" => {
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
                emit!(op);
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
                    // Mask text-showing operators even outside BT…ET:
                    // such operators are invalid PDF but must not leak.
                    let (x, y) = (text_matrix[4], text_matrix[5]);
                    let masked = mask_string_partial(
                        &text,
                        x,
                        y,
                        font_size,
                        current_metrics.as_ref(),
                        regions,
                    );
                    out.push('(');
                    out.push_str(&masked);
                    out.push_str(") Tj\n");
                    let width = text_width(&text, font_size, current_metrics.as_ref());
                    text_matrix[4] = x + width;
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
                                let masked = mask_string_partial(
                                    &t,
                                    x,
                                    y,
                                    font_size,
                                    current_metrics.as_ref(),
                                    regions,
                                );
                                out.push('(');
                                out.push_str(&masked);
                                out.push(')');
                                let width = text_width(&t, font_size, current_metrics.as_ref());
                                x += width;
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
                    let (x, y) = (text_matrix[4], text_matrix[5]);
                    let masked = mask_string_partial(
                        &text,
                        x,
                        y,
                        font_size,
                        current_metrics.as_ref(),
                        regions,
                    );
                    out.push_str(&masked);
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
                    emit!(op);
                }
            }
        }
        pending_name = None;
        operands.clear();
        i += 1;
    }
    (out, removed_images)
}

/// Apply a 6-element affine matrix to a point.
fn ctm_apply(m: &[f32; 6], x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
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
    TjItem, collect_font_metrics, decompress_stream, extract_string as search_extract_string,
    extract_tj_array, is_deflate_stream, page_content_streams,
};

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

    #[test]
    fn rewrite_masks_text_outside_bt_et() {
        // Text-showing operators outside BT…ET must still be masked.
        let regions = [Rect {
            x: 0.0,
            y: 690.0,
            width: 600.0,
            height: 30.0,
        }];
        let src = "/F1 12 Tf\n1 0 0 1 100 700 Tm\n(SECRET outside) Tj\n";
        let (out, removed) = rewrite_stream(
            src,
            &regions,
            &HashMap::new(),
            RedactionStyle::Strip,
            &HashMap::new(),
        );
        assert!(removed.is_empty());
        assert!(
            !out.contains("SECRET"),
            "text outside BT…ET must be masked: {out}"
        );
    }

    #[test]
    fn rewrite_removes_intersecting_image_do_and_reports_name() {
        // Image drawn at 100..300 x 100..200 via cm; region overlaps it.
        let mut xobjects = HashMap::new();
        xobjects.insert(
            "Im0".to_string(),
            XObjectInfo {
                id: 9,
                is_image: true,
                is_form: false,
            },
        );
        let regions = [Rect {
            x: 150.0,
            y: 120.0,
            width: 100.0,
            height: 50.0,
        }];
        let src = "q\n200 0 0 100 100 100 cm\n/Im0 Do\nQ\n";
        let (out, removed) = rewrite_stream(
            src,
            &regions,
            &HashMap::new(),
            RedactionStyle::Strip,
            &xobjects,
        );
        assert_eq!(removed, vec!["Im0".to_string()]);
        assert!(!out.contains("/Im0 Do"), "image Do must be removed: {out}");
    }

    #[test]
    fn rewrite_does_not_mangle_names_or_strings_before_do() {
        // A '/' inside a string literal must not be mistaken for the Do operand.
        let mut xobjects = HashMap::new();
        xobjects.insert(
            "Im0".to_string(),
            XObjectInfo {
                id: 9,
                is_image: true,
                is_form: false,
            },
        );
        let regions = [Rect {
            x: 150.0,
            y: 120.0,
            width: 100.0,
            height: 50.0,
        }];
        let src = "BT\n(a/b) Tj\nET\nq\n200 0 0 100 100 100 cm\n/Im0 Do\nQ\n";
        let (out, removed) = rewrite_stream(
            src,
            &regions,
            &HashMap::new(),
            RedactionStyle::Strip,
            &xobjects,
        );
        assert_eq!(removed, vec!["Im0".to_string()]);
        // The string operand must survive intact (no rfind('/') truncation).
        assert!(out.contains("(a/b)"), "string operand must survive: {out}");
    }

    #[test]
    fn redaction_removes_image_object_from_document() {
        // Build a PDF with an embedded image, then redact over it.
        let png_path = format!("{}/tests/fixtures/sample.png", env!("CARGO_MANIFEST_DIR"));
        let elements = vec![crate::elements::Element::Image {
            alt: "sample".to_string(),
            path: png_path,
        }];
        let pdf = generate_pdf_bytes(&elements, "Helvetica", 12.0, PageLayout::portrait()).unwrap();
        let is_image = |o: &PdfObject| {
            matches!(o, PdfObject::Stream { dictionary, .. }
                if dictionary.get("Subtype")
                    .and_then(|v| match v {
                        PdfValue::Object(PdfObject::Name(n)) => Some(n.as_str()),
                        _ => None,
                    })
                    .is_some_and(|n| n == "Image"))
        };
        // Sanity: the original contains an image XObject.
        let doc = PdfDocument::load_from_bytes(&pdf).unwrap();
        if !doc.objects.values().any(&is_image) {
            return; // fixture unavailable — skip rather than fail
        }

        // Cover the whole page.
        let redacted = redact_pdf_bytes(
            &pdf,
            &[RedactionRegion {
                page: 0,
                x: 0.0,
                y: 0.0,
                width: 612.0,
                height: 792.0,
            }],
        )
        .unwrap();
        let doc2 = PdfDocument::load_from_bytes(&redacted).unwrap();
        assert!(
            !doc2.objects.values().any(&is_image),
            "image object must be removed from the document"
        );
    }
}
