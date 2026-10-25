//! Applying redactions to a document.
//!
//! Walks the pages, rewrites every content stream that can carry
//! text-showing operators (page content, form XObjects, annotation
//! appearance streams) and drops the image objects that the redaction
//! removed.

use std::collections::HashMap;

use super::document::{
    collect_annotation_ap_streams, collect_xobjects, is_referenced, remove_xobject_entries,
};
use super::model::{RedactionRegion, RedactionStyle};
use super::rewrite::rewrite_stream;
use crate::compression::compress_deflate;
use crate::error::{PdfError, Result};
use crate::pdf::{PdfDocument, PdfObject, PdfValue};
use crate::search::{
    Rect, collect_font_metrics, decompress_stream, is_deflate_stream, page_content_streams,
};

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
        return Err(PdfError::InvalidPdf("PDF has no pages".into()));
    }

    // Bucket regions by page for O(1) lookup.
    let mut by_page: HashMap<usize, Vec<Rect>> = HashMap::new();
    for r in regions {
        if r.page >= pages.len() {
            return Err(PdfError::PageNotFound(r.page));
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
