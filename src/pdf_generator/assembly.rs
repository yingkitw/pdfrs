//! PDF object assembly: turning rendered page streams into a PDF file.
//!
//! Builds the per-page content streams and dictionaries, the `/Pages` tree,
//! the optional tagged-PDF structures, the `/Outlines` bookmark tree and the
//! catalog, then serialises the result.

use super::AccessibilityOptions;
use super::content_stream::{
    FONT_COURIER, FONT_HELVETICA, FONT_HELVETICA_BOLD, FONT_HELVETICA_BOLD_OBLIQUE,
    FONT_HELVETICA_OBLIQUE, OutlineDest,
};
use super::fonts::add_shared_font_resources;
use super::layout::PageLayout;
use super::objects::PdfGenerator;
use super::unicode_support::UnicodeFontEncoder;
use crate::error::{PdfError, Result};
use crate::image::{self, ImageInfo};
use crate::pdf_ops::escape_pdf_meta;

/// Assemble final PDF bytes from per-page content streams
#[allow(clippy::too_many_arguments)]
pub(super) fn assemble_pdf_bytes(
    page_streams: &[Vec<u8>],
    _font: &str,
    layout: &PageLayout,
    unicode_font: Option<(
        &[u8],
        &UnicodeFontEncoder,
        &std::collections::BTreeSet<char>,
    )>,
    compression_level: Option<u8>,
    accessibility: Option<&AccessibilityOptions>,
    outlines: &[OutlineDest],
    images: &[(String, ImageInfo)],
) -> Result<Vec<u8>> {
    let mut generator = PdfGenerator::new().with_version(layout.version);

    let font_ids = add_shared_font_resources(&mut generator, unicode_font);

    // Shared image XObjects (referenced from content as /ImN Do)
    let mut xobject_resource = String::new();
    if !images.is_empty() {
        let mut parts = Vec::new();
        let mut embed_errors = Vec::new();
        for (name, info) in images {
            match image::create_image_object(&mut generator, info.clone()) {
                Ok(id) => parts.push(format!("/{} {} 0 R", name, id)),
                Err(e) => embed_errors.push(format!("{}: {}", name, e)),
            }
        }
        if !embed_errors.is_empty() {
            return Err(PdfError::Image(format!(
                "Failed to embed image XObject(s):\n  - {}",
                embed_errors.join("\n  - ")
            )));
        }
        if !parts.is_empty() {
            xobject_resource = format!("/XObject << {} >> ", parts.join(" "));
        }
    }

    let mut page_ids = Vec::new();

    // Objects now are: shared fonts (+ images), then each page contributes
    // [content_stream, page_dict], then pages object and catalog object.
    let per_page_objects = 2u32;
    let pages_obj_id = generator.next_id + per_page_objects * page_streams.len() as u32;

    for page_stream in page_streams {
        let (dict, data) = if let Some(level) = compression_level {
            match crate::compression::compress_deflate_with_level(page_stream, level) {
                Ok(compressed) if compressed.len() < page_stream.len() => (
                    format!("<< /Length {} /Filter /FlateDecode >>\n", compressed.len()),
                    compressed,
                ),
                _ => (
                    format!("<< /Length {} >>\n", page_stream.len()),
                    page_stream.clone(),
                ),
            }
        } else {
            (
                format!("<< /Length {} >>\n", page_stream.len()),
                page_stream.clone(),
            )
        };
        let content_id = generator.add_stream_object(dict, data);

        let page_dict = format!(
            "<< /Type /Page\n\
             /Parent {} 0 R\n\
             /MediaBox [0 0 {} {}]\n\
             /Contents {} 0 R\n\
             /Resources << /Font << \
                 /{} {} 0 R \
                 /{} {} 0 R \
                 /{} {} 0 R \
                 /{} {} 0 R \
                 /{} {} 0 R \
             >> {}>>\n\
             >>\n",
            pages_obj_id,
            layout.width,
            layout.height,
            content_id,
            FONT_HELVETICA,
            font_ids.helvetica,
            FONT_HELVETICA_BOLD,
            font_ids.helvetica_bold,
            FONT_HELVETICA_OBLIQUE,
            font_ids.helvetica_oblique,
            FONT_HELVETICA_BOLD_OBLIQUE,
            font_ids.helvetica_bold_oblique,
            FONT_COURIER,
            font_ids.courier,
            xobject_resource,
        );
        let page_id = generator.add_object(page_dict);
        page_ids.push(page_id);
    }

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let pages_dict = format!(
        "<< /Type /Pages\n\
         /Kids [{}]\n\
         /Count {}\n\
         >>\n",
        kids.join(" "),
        page_ids.len()
    );
    let actual_pages_id = generator.add_object(pages_dict);
    assert_eq!(actual_pages_id, pages_obj_id);

    // Build tagged PDF structures if accessibility options are provided
    let mut struct_tree_id = None;
    if let Some(opts) = accessibility {
        if opts.tagged_pdf {
            // Create a simple structure tree root
            let struct_tree_dict = format!(
                "<< /Type /StructTreeRoot\n\
                 /K [ << /Type /StructElem /S /Document /P {} 0 R >> ]\n\
                 >>\n",
                generator.next_id // placeholder parent, points to self in this minimal tree
            );
            struct_tree_id = Some(generator.add_object(struct_tree_dict));
        }

        // Info dictionary with title if provided
        if let Some(title) = &opts.title {
            let info_dict = format!(
                "<< /Title ({})\n\
                 /Producer (pdfrs)\n\
                 >>\n",
                escape_pdf_meta(title)
            );
            let info_id = generator.add_object(info_dict);
            generator.info_id = Some(info_id);
        }
    }

    // Build catalog with optional tagged PDF entries and outlines
    let mut catalog_entries = format!("/Pages {} 0 R\n", actual_pages_id);
    if let Some(opts) = accessibility
        && opts.tagged_pdf
    {
        catalog_entries.push_str("/MarkInfo << /Marked true >>\n");
        catalog_entries.push_str(&format!("/Lang ({})\n", escape_pdf_meta(&opts.language)));
        if let Some(st_id) = struct_tree_id {
            catalog_entries.push_str(&format!("/StructTreeRoot {} 0 R\n", st_id));
        }
    }

    if let Some(outlines_id) = add_outline_tree(&mut generator, &page_ids, outlines) {
        catalog_entries.push_str(&format!("/Outlines {} 0 R\n", outlines_id));
        catalog_entries.push_str("/PageMode /UseOutlines\n");
    }

    let catalog_dict = format!(
        "<< /Type /Catalog\n\
         {}\
         >>\n",
        catalog_entries
    );
    generator.add_object(catalog_dict);

    Ok(generator.generate())
}

/// Build a flat `/Outlines` tree linking each heading to its page.
fn add_outline_tree(
    generator: &mut PdfGenerator,
    page_ids: &[u32],
    outlines: &[OutlineDest],
) -> Option<u32> {
    if outlines.is_empty() || page_ids.is_empty() {
        return None;
    }

    let n = outlines.len() as u32;
    let first_item_id = generator.next_id;
    let last_item_id = first_item_id + n - 1;
    let root_id = first_item_id + n; // appended after all item objects

    for (i, entry) in outlines.iter().enumerate() {
        let page_idx = entry.page_index.min(page_ids.len() - 1);
        let page_ref = page_ids[page_idx];
        let mut dict = format!(
            "<< /Title ({})\n/Parent {} 0 R\n/Dest [{} 0 R /XYZ null {:.2} null]\n",
            escape_pdf_meta(&entry.title),
            root_id,
            page_ref,
            entry.y,
        );
        if i > 0 {
            dict.push_str(&format!("/Prev {} 0 R\n", first_item_id + (i as u32) - 1));
        }
        if i + 1 < outlines.len() {
            dict.push_str(&format!("/Next {} 0 R\n", first_item_id + (i as u32) + 1));
        }
        dict.push_str(">>\n");
        let actual = generator.add_object(dict);
        debug_assert_eq!(actual, first_item_id + i as u32);
    }

    let root_dict = format!(
        "<< /Type /Outlines\n/First {} 0 R\n/Last {} 0 R\n/Count {}\n>>\n",
        first_item_id, last_item_id, n
    );
    let actual_root = generator.add_object(root_dict);
    debug_assert_eq!(actual_root, root_id);
    Some(actual_root)
}
