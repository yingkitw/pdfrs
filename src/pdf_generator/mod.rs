//! PDF generation from structured document elements
//!
//! This module is the core PDF engine. It takes a vector of [`Element`]s
//! (produced by [`elements::parse_markdown`][`crate::elements::parse_markdown`])
//! and renders them into a standards-compliant PDF byte stream.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use pdfrs::elements;
//! use pdfrs::pdf_generator::{generate_pdf_bytes, PageLayout};
//!
//! let elements = elements::parse_markdown("# Hello\n\nWorld");
//! let layout = PageLayout::portrait();
//! let pdf = generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();
//! ```
//!
//! # Key types
//!
//! - [`PageLayout`] — page size, orientation, and margins
//! - [`PageOrientation`] — `Portrait` or `Landscape`
//! - [`generate_pdf_bytes`] — in-memory PDF generation
//! - [`create_pdf_from_elements_with_layout`] — write to file
//! - [`crate::streaming::StreamingPdfGenerator`] — incremental generation for large documents
//!
//! # Unicode font embedding
//!
//! When the document contains non-ASCII characters, the generator
//! automatically embeds a TrueType font (configurable via the
//! `PDFRS_UNICODE_FONT_PATH` environment variable). Font subsetting
//! can be enabled via [`crate::optimization::OptimizationSettings`] in the [`optimization`](crate::optimization)
//! module.

use crate::elements::Element;
use crate::image::{self, ImageInfo};
use crate::pdf_ops::escape_pdf_meta;
use anyhow::Result;
use std::path::PathBuf;

mod accessibility;
mod code_highlight;
mod content_stream;
mod layout;
mod math_layout;
mod text_support;
mod unicode_support;

pub use content_stream::OutlineDest;
pub(crate) use content_stream::{
    ContentStreamBuilder, prepare_elements_for_render, render_elements_to_builder,
};
use content_stream::{
    FONT_COURIER, FONT_HELVETICA, FONT_HELVETICA_BOLD, FONT_HELVETICA_BOLD_OBLIQUE,
    FONT_HELVETICA_OBLIQUE,
};
pub use content_stream::{
    create_pdf, create_pdf_from_elements, create_pdf_from_elements_with_layout,
    create_pdf_from_elements_with_layout_and_compression, create_pdf_with_options,
};
pub use layout::{Color, PageLayout, PageOrientation, PdfVersion, TextAlign};
use layout::{collect_unicode_chars, document_requires_unicode};

pub(crate) use text_support::{escape_pdf_string, render_math_text};
#[cfg(test)]
use unicode_support::prepare_unicode_font_support;
use unicode_support::{UnicodeFontEncoder, prepare_unicode_font_support_with_subsetting};

// --- Low-level PDF object model ---

pub struct PdfGenerator {
    pub objects: Vec<PdfObj>,
    pub next_id: u32,
    pub info_id: Option<u32>,
    pub version: PdfVersion,
}

#[derive(Debug)]
pub struct PdfObj {
    pub id: u32,
    pub generation: u32,
    pub content: String,
    pub is_stream: bool,
    pub stream_data: Option<Vec<u8>>,
}

impl Default for PdfGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfGenerator {
    pub fn new() -> Self {
        PdfGenerator {
            objects: Vec::new(),
            next_id: 1,
            info_id: None,
            version: PdfVersion::default(),
        }
    }

    pub fn with_version(mut self, version: PdfVersion) -> Self {
        self.version = version;
        self
    }

    pub fn add_object(&mut self, content: String) -> u32 {
        let id = self.next_id;
        self.objects.push(PdfObj {
            id,
            generation: 0,
            content,
            is_stream: false,
            stream_data: None,
        });
        self.next_id += 1;
        id
    }

    pub fn add_stream_object(&mut self, dictionary: String, data: Vec<u8>) -> u32 {
        let id = self.next_id;
        self.objects.push(PdfObj {
            id,
            generation: 0,
            content: dictionary,
            is_stream: true,
            stream_data: Some(data),
        });
        self.next_id += 1;
        id
    }

    pub fn generate(&self) -> Vec<u8> {
        let mut pdf = Vec::new();

        // PDF header
        pdf.extend_from_slice(self.version.header());

        // Calculate offsets for xref table
        let mut offsets = Vec::new();
        let mut current_offset = pdf.len() as u32;

        // Write objects and collect offsets
        for obj in &self.objects {
            offsets.push(current_offset);
            let obj_header = format!("{} {} obj\n", obj.id, obj.generation);
            pdf.extend_from_slice(obj_header.as_bytes());
            pdf.extend_from_slice(obj.content.as_bytes());

            if obj.is_stream
                && let Some(data) = &obj.stream_data
            {
                pdf.extend_from_slice(b"stream\n");
                pdf.extend_from_slice(data);
                pdf.extend_from_slice(b"\nendstream\n");
            }

            pdf.extend_from_slice(b"endobj\n");
            current_offset = pdf.len() as u32;
        }

        // xref table
        let xref_offset = pdf.len() as u32;
        pdf.extend_from_slice(format!("xref\n0 {}\n", self.objects.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");

        for offset in offsets {
            pdf.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
        }

        // trailer
        pdf.extend_from_slice(b"trailer\n");
        pdf.extend_from_slice(b"<<\n");
        pdf.extend_from_slice(format!("/Size {}\n", self.objects.len() + 1).as_bytes());
        if !self.objects.is_empty() {
            pdf.extend_from_slice(format!("/Root {} 0 R\n", self.objects.len()).as_bytes());
        }
        if let Some(info_id) = self.info_id {
            pdf.extend_from_slice(format!("/Info {} 0 R\n", info_id).as_bytes());
        }
        pdf.extend_from_slice(b">>\n");
        pdf.extend_from_slice(b"startxref\n");
        pdf.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        pdf.extend_from_slice(b"%%EOF\n");

        pdf
    }
}

#[derive(Debug, Clone, Copy)]
struct FontResourceIds {
    helvetica: u32,
    helvetica_bold: u32,
    helvetica_oblique: u32,
    helvetica_bold_oblique: u32,
    courier: u32,
}

fn add_shared_font_resources(
    generator: &mut PdfGenerator,
    unicode_font: Option<(
        &[u8],
        &UnicodeFontEncoder,
        &std::collections::BTreeSet<char>,
    )>,
) -> FontResourceIds {
    let helvetica_id = if let Some((bytes, encoder, chars)) = unicode_font {
        let font_file_id =
            generator.add_stream_object(format!("<< /Length {} >>\n", bytes.len()), bytes.to_vec());

        let descriptor_id = generator.add_object(format!(
            "<< /Type /FontDescriptor\n/FontName /UnicodeTT\n/Flags 4\n/FontBBox [0 -200 1000 900]\n/ItalicAngle 0\n/Ascent 800\n/Descent -200\n/CapHeight 700\n/StemV 80\n/MissingWidth 500\n/FontFile2 {} 0 R\n>>\n",
            font_file_id
        ));

        let widths = encoder.build_cid_widths_array(chars);
        let cid_font_id = generator.add_object(format!(
            "<< /Type /Font\n/Subtype /CIDFontType2\n/BaseFont /UnicodeTT\n/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >>\n/FontDescriptor {} 0 R\n/DW 500\n/W {}\n/CIDToGIDMap /Identity\n>>\n",
            descriptor_id, widths
        ));

        let tounicode = encoder.build_tounicode_cmap(chars);
        let tounicode_id =
            generator.add_stream_object(format!("<< /Length {} >>\n", tounicode.len()), tounicode);

        generator.add_object(format!(
            "<< /Type /Font\n/Subtype /Type0\n/BaseFont /UnicodeTT\n/Encoding /Identity-H\n/DescendantFonts [{} 0 R]\n/ToUnicode {} 0 R\n>>\n",
            cid_font_id, tounicode_id
        ))
    } else {
        generator.add_object(format!(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
            FONT_HELVETICA
        ))
    };

    let (helvetica_bold_id, helvetica_oblique_id, helvetica_bold_oblique_id) =
        if unicode_font.is_some() {
            // Reuse the same embedded Type0/CIDFont for style variants to keep
            // unicode/maths glyph coverage in italic/bold rendering paths.
            (helvetica_id, helvetica_id, helvetica_id)
        } else {
            (
                generator.add_object(format!(
                    "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
                    FONT_HELVETICA_BOLD
                )),
                generator.add_object(format!(
                    "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
                    FONT_HELVETICA_OBLIQUE
                )),
                generator.add_object(format!(
                    "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
                    FONT_HELVETICA_BOLD_OBLIQUE
                )),
            )
        };

    let courier_id = generator.add_object(format!(
        "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
        FONT_COURIER
    ));

    FontResourceIds {
        helvetica: helvetica_id,
        helvetica_bold: helvetica_bold_id,
        helvetica_oblique: helvetica_oblique_id,
        helvetica_bold_oblique: helvetica_bold_oblique_id,
        courier: courier_id,
    }
}

/// Internal helper for generating PDF bytes with optional font subsetting.
pub(crate) fn generate_pdf_bytes_internal(
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
    compression_level: Option<u8>,
    subset_fonts: bool,
    accessibility: Option<&AccessibilityOptions>,
) -> Result<Vec<u8>> {
    generate_pdf_bytes_internal_with_base(
        elements,
        font,
        base_font_size,
        layout,
        compression_level,
        subset_fonts,
        accessibility,
        None,
    )
}

/// Same as [`generate_pdf_bytes_internal`] with an optional base directory for
/// resolving relative markdown image paths.
pub(crate) fn generate_pdf_bytes_internal_with_base(
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
    compression_level: Option<u8>,
    subset_fonts: bool,
    accessibility: Option<&AccessibilityOptions>,
    image_base_dir: Option<PathBuf>,
) -> Result<Vec<u8>> {
    // Dry-run TOC expansion does not need a Unicode encoder.
    let (prepared, citation_defs) = prepare_elements_for_render(
        elements,
        base_font_size,
        layout,
        None,
        image_base_dir.clone(),
    );

    let used_chars = if document_requires_unicode(&prepared) {
        Some(collect_unicode_chars(&prepared))
    } else {
        None
    };
    let unicode_font_support = if used_chars.is_some() {
        let chars = if subset_fonts {
            used_chars.as_ref()
        } else {
            None
        };
        prepare_unicode_font_support_with_subsetting(chars)
    } else {
        None
    };
    let unicode_font_encoder = unicode_font_support
        .as_ref()
        .map(|(_, encoder)| encoder.clone());
    let show_page_numbers = true;
    let mut builder = ContentStreamBuilder::new(
        base_font_size,
        show_page_numbers,
        layout,
        unicode_font_encoder,
        image_base_dir,
    );
    builder.citation_defs = citation_defs;
    render_elements_to_builder(&mut builder, &prepared, base_font_size);
    if !builder.image_errors.is_empty() {
        return Err(anyhow::anyhow!(
            "Image embedding failed:\n  - {}",
            builder.image_errors.join("\n  - ")
        ));
    }
    let (page_streams, outlines, images) = builder.finish();
    let unicode_arg = match (&unicode_font_support, &used_chars) {
        (Some((bytes, encoder)), Some(chars)) => Some((bytes.as_slice(), encoder, chars)),
        _ => None,
    };
    Ok(assemble_pdf_bytes(
        &page_streams,
        font,
        &layout,
        unicode_arg,
        compression_level,
        accessibility,
        &outlines,
        &images,
    )?)
}

/// Generate PDF bytes from elements (library API — no filesystem access needed)
pub fn generate_pdf_bytes(
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
) -> Result<Vec<u8>> {
    generate_pdf_bytes_internal(elements, font, base_font_size, layout, None, false, None)
}

/// Same as [`generate_pdf_bytes`], resolving relative image paths against `image_base_dir`.
pub fn generate_pdf_bytes_with_image_base(
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
    image_base_dir: impl Into<PathBuf>,
) -> Result<Vec<u8>> {
    generate_pdf_bytes_internal_with_base(
        elements,
        font,
        base_font_size,
        layout,
        None,
        false,
        None,
        Some(image_base_dir.into()),
    )
}

/// Same as `generate_pdf_bytes` but with optional stream compression
pub fn generate_pdf_bytes_with_compression(
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
    compression_level: Option<u8>,
) -> Result<Vec<u8>> {
    generate_pdf_bytes_internal(
        elements,
        font,
        base_font_size,
        layout,
        compression_level,
        false,
        None,
    )
}

/// Generate a tagged/accessible PDF with PDF/UA structural support.
///
/// When `options.tagged_pdf` is true, the output includes:
/// - `/MarkInfo << /Marked true >>` in the catalog
/// - `/StructTreeRoot` with a document-level structure tree
/// - `/Lang` attribute on the catalog
/// - `/Title` in the Info dictionary (if provided)
///
/// # Example
/// ```rust,no_run
/// use pdfrs::pdf_generator::{generate_tagged_pdf_bytes, AccessibilityOptions, PageLayout};
/// use pdfrs::elements::Element;
///
/// let elements = vec![Element::Paragraph { text: "Hello".into() }];
/// let opts = AccessibilityOptions::new()
///     .with_tagged_pdf(true)
///     .with_title("My Document".to_string());
/// let bytes = generate_tagged_pdf_bytes(&elements, "Helvetica", 12.0, PageLayout::portrait(), opts).unwrap();
/// ```
pub fn generate_tagged_pdf_bytes(
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
    options: AccessibilityOptions,
) -> Result<Vec<u8>> {
    generate_pdf_bytes_internal(
        elements,
        font,
        base_font_size,
        layout,
        None,
        false,
        Some(&options),
    )
}

/// Render only a specific page range from elements into a standalone PDF.
///
/// Pages are 0-indexed; `range` follows Rust's `Range<usize>` semantics
/// (start inclusive, end exclusive). This is useful for previewing or
/// extracting a subset of pages without regenerating the entire document.
///
/// # Example
/// ```rust,no_run
/// use pdfrs::pdf_generator::{PageLayout, render_page_range};
/// use pdfrs::elements::Element;
///
/// let elements = vec![
///     Element::Paragraph { text: "Page 1".into() },
///     Element::PageBreak,
///     Element::Paragraph { text: "Page 2".into() },
///     Element::PageBreak,
///     Element::Paragraph { text: "Page 3".into() },
/// ];
/// let bytes = render_page_range(&elements, "Helvetica", 12.0, PageLayout::portrait(), 1..3).unwrap();
/// // bytes now contains a 2-page PDF with "Page 2" and "Page 3"
/// ```
pub fn render_page_range(
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
    range: std::ops::Range<usize>,
) -> Result<Vec<u8>> {
    let used_chars = if document_requires_unicode(elements) {
        Some(collect_unicode_chars(elements))
    } else {
        None
    };
    let unicode_font_support = if used_chars.is_some() {
        prepare_unicode_font_support_with_subsetting(used_chars.as_ref())
    } else {
        None
    };
    let unicode_font_encoder = unicode_font_support
        .as_ref()
        .map(|(_, encoder)| encoder.clone());
    let show_page_numbers = true;
    let mut builder = ContentStreamBuilder::new(
        base_font_size,
        show_page_numbers,
        layout,
        unicode_font_encoder,
        None,
    );
    render_elements_to_builder(&mut builder, elements, base_font_size);
    let (all_page_streams, outlines, images) = builder.finish();

    if range.start >= all_page_streams.len() {
        anyhow::bail!(
            "Start page {} exceeds total pages {}",
            range.start,
            all_page_streams.len()
        );
    }
    let end = range.end.min(all_page_streams.len());
    let selected = &all_page_streams[range.start..end];
    // Remap outline page indices into the selected range
    let filtered: Vec<OutlineDest> = outlines
        .into_iter()
        .filter(|o| o.page_index >= range.start && o.page_index < end)
        .map(|mut o| {
            o.page_index -= range.start;
            o
        })
        .collect();

    let unicode_arg = match (&unicode_font_support, &used_chars) {
        (Some((bytes, encoder)), Some(chars)) => Some((bytes.as_slice(), encoder, chars)),
        _ => None,
    };
    Ok(assemble_pdf_bytes(
        selected,
        font,
        &layout,
        unicode_arg,
        None,
        None,
        &filtered,
        &images,
    )?)
}

/// Assemble final PDF bytes from per-page content streams
fn assemble_pdf_bytes(
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
            return Err(anyhow::anyhow!(
                "Failed to embed image XObject(s):\n  - {}",
                embed_errors.join("\n  - ")
            ));
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

pub use accessibility::{
    AccessibilityOptions, StructureElement, StructureType, element_to_structure,
};

#[cfg(test)]
mod accessibility_tests {
    use super::*;

    #[test]
    fn test_accessibility_options_default() {
        let opts = AccessibilityOptions::default();
        assert!(!opts.tagged_pdf);
        assert_eq!(opts.language, "en");
        assert!(opts.title.is_none());
    }

    #[test]
    fn test_accessibility_options_builder() {
        let opts = AccessibilityOptions::new()
            .with_tagged_pdf(true)
            .with_language("en-US".to_string())
            .with_title("My Document".to_string());

        assert!(opts.tagged_pdf);
        assert_eq!(opts.language, "en-US");
        assert_eq!(opts.title, Some("My Document".to_string()));
    }

    #[test]
    fn test_structure_type_names() {
        assert_eq!(StructureType::Document.as_pdf_name(), "Document");
        assert_eq!(StructureType::P.as_pdf_name(), "P");
        assert_eq!(StructureType::H1.as_pdf_name(), "H1");
        assert_eq!(StructureType::Figure.as_pdf_name(), "Figure");
    }

    #[test]
    fn test_structure_element_builder() {
        let elem = StructureElement::new(StructureType::P)
            .with_alt_text("A paragraph".to_string())
            .with_actual_text("This is the actual text".to_string());

        assert_eq!(elem.struct_type, StructureType::P);
        assert_eq!(elem.alt_text, Some("A paragraph".to_string()));
        assert_eq!(
            elem.actual_text,
            Some("This is the actual text".to_string())
        );
    }

    #[test]
    fn test_structure_element_with_children() {
        let mut parent = StructureElement::new(StructureType::L);
        parent.add_child(StructureElement::new(StructureType::LI));
        parent.add_child(StructureElement::new(StructureType::LI));

        assert_eq!(parent.children.len(), 2);
    }

    #[test]
    fn test_element_to_structure_heading() {
        let elem = Element::Heading {
            level: 1,
            text: "Hello".into(),
        };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::H1);
        assert_eq!(struct_elem.actual_text, Some("Hello".to_string()));
    }

    #[test]
    fn test_element_to_structure_paragraph() {
        let elem = Element::Paragraph {
            text: "Test paragraph".into(),
        };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::P);
        assert_eq!(struct_elem.actual_text, Some("Test paragraph".to_string()));
    }

    #[test]
    fn test_element_to_structure_code() {
        let elem = Element::CodeBlock {
            language: "rust".into(),
            code: "fn main() {}".into(),
        };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::Code);
        assert_eq!(struct_elem.actual_text, Some("fn main() {}".to_string()));
    }

    #[test]
    fn test_render_math_text_nested_frac_and_left_right() {
        let quadratic = render_math_text(r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}");
        assert!(
            !quadratic.contains('\\'),
            "should not leave raw LaTeX commands: {}",
            quadratic
        );
        assert!(quadratic.contains('±'), "rendered: {}", quadratic);
        assert!(quadratic.contains('√'), "rendered: {}", quadratic);
        assert!(
            quadratic.contains(")/(") || quadratic.contains('⁄'),
            "fraction should render: {}",
            quadratic
        );

        let gaussian = render_math_text(r"\exp\left(-\frac{(x - \mu)^2}{2\sigma^2}\right)");
        assert!(
            !gaussian.contains("≤ft") && !gaussian.contains("\\left"),
            "\\le must not eat \\left: {}",
            gaussian
        );
        assert!(gaussian.contains("exp"), "rendered: {}", gaussian);
    }

    #[test]
    fn test_render_math_text_matrices() {
        let m = render_math_text(r"\begin{bmatrix} a & b \\ c & d \end{bmatrix}");
        assert!(m.contains('[') && m.contains(']'), "rendered: {}", m);
        assert!(m.contains('a') && m.contains('d'), "rendered: {}", m);
        assert!(!m.contains("begin"), "rendered: {}", m);

        let v = render_math_text(r"\begin{vmatrix} \hat{i} & \hat{j} \\ a_1 & a_2 \end{vmatrix}");
        assert!(v.contains('|'), "rendered: {}", v);
        assert!(!v.contains("begin"), "rendered: {}", v);
    }

    #[test]
    fn test_render_math_text_uses_unicode_symbols() {
        let rendered = render_math_text(r"\sum_{i=1}^{n} i \leq n^2 \approx \infty + \sqrt{x}");
        assert!(rendered.contains('∑'));
        assert!(rendered.contains('≤'));
        assert!(rendered.contains('≈'));
        assert!(rendered.contains('∞'));
        assert!(rendered.contains('√'));
    }

    #[test]
    fn test_render_math_text_handles_unbraced_limits() {
        let rendered = render_math_text(r"\int_0^1 x^2 dx + \sum_i^n a_i");
        assert!(
            rendered.contains("∫₀¹") || rendered.contains("∫[0→1]"),
            "rendered: {}",
            rendered
        );
        assert!(
            rendered.contains("∑ᵢⁿ") || rendered.contains("∑[i→n]"),
            "rendered: {}",
            rendered
        );
        assert!(
            rendered.contains("x²") || rendered.contains("x^(2)"),
            "rendered: {}",
            rendered
        );
        assert!(
            rendered.contains("aᵢ") || rendered.contains("a_(i)"),
            "rendered: {}",
            rendered
        );
    }

    #[test]
    fn test_render_math_text_handles_lim_and_to_arrow() {
        let rendered = render_math_text(r"\lim_{x\to0} \frac{\sin x}{x}");
        assert!(rendered.contains("lim(x→0)"), "rendered: {}", rendered);
        assert!(
            rendered.contains("(sin x)/(x)") || rendered.contains("sin x⁄x"),
            "rendered: {}",
            rendered
        );
    }

    #[test]
    fn test_render_math_text_handles_notin_without_partial_in_replacement() {
        let rendered = render_math_text(r"x \notin A");
        assert!(rendered.contains("∉"), "rendered: {}", rendered);
        assert!(!rendered.contains("∈"), "rendered: {}", rendered);
    }

    #[test]
    fn test_render_math_text_handles_set_logic_and_mathbb_symbols() {
        let rendered = render_math_text(
            r"\forall x \in \mathbb{R}, x \subseteq A \land x \notin B \Rightarrow \therefore x \in \mathbb{N}",
        );
        assert!(rendered.contains("∀"), "rendered: {}", rendered);
        assert!(rendered.contains("∈"), "rendered: {}", rendered);
        assert!(rendered.contains("ℝ"), "rendered: {}", rendered);
        assert!(rendered.contains("⊆"), "rendered: {}", rendered);
        assert!(rendered.contains("∧"), "rendered: {}", rendered);
        assert!(rendered.contains("∉"), "rendered: {}", rendered);
        assert!(rendered.contains("⇒"), "rendered: {}", rendered);
        assert!(rendered.contains("∴"), "rendered: {}", rendered);
        assert!(rendered.contains("ℕ"), "rendered: {}", rendered);
    }

    #[test]
    fn test_render_math_text_complex_expressions_with_unicode() {
        let expr1 = render_math_text(r"\int_0^1 x^2 dx + \sum_{i=1}^{n} a_i");
        assert!(
            expr1.contains("∫₀¹"),
            "Should render integral with subscript/superscript: {}",
            expr1
        );
        assert!(expr1.contains("∑"), "Should contain sum symbol: {}", expr1);
        assert!(
            expr1.contains("∑[i=1→n]") || expr1.contains("∑ᵢ"),
            "Sum limits should be readable: {}",
            expr1
        );
        assert!(expr1.contains("x²"), "Should render x squared: {}", expr1);

        let expr2 = render_math_text(r"\prod_{k=1}^{m} b_k");
        assert!(
            expr2.contains("∏"),
            "Should contain product symbol: {}",
            expr2
        );
        assert!(
            expr2.contains("∏[k=1→m]") || expr2.contains("∏ₖ"),
            "Product limits should be readable: {}",
            expr2
        );
        assert!(
            expr2.contains("bₖ") || expr2.contains("b_(k)"),
            "Should render b subscript k: {}",
            expr2
        );

        let expr3 = render_math_text(
            r"\forall x \in \mathbb{R}, x \geq 0 \Rightarrow \sqrt{x} \in \mathbb{R}",
        );
        assert!(expr3.contains("∀"), "Should contain forall: {}", expr3);
        assert!(expr3.contains("∈"), "Should contain element of: {}", expr3);
        assert!(
            expr3.contains("ℝ"),
            "Should contain real numbers: {}",
            expr3
        );
        assert!(
            expr3.contains("≥"),
            "Should contain greater or equal: {}",
            expr3
        );
        assert!(expr3.contains("⇒"), "Should contain implies: {}", expr3);
        assert!(expr3.contains("√"), "Should contain square root: {}", expr3);
    }

    #[test]
    fn test_multi_column_layout_flows_across_columns() {
        let md = r#"<!-- columns:2 -->

## Column Demo

Paragraph one fills the first column with enough words that wrapping and
column advancement can be exercised on a letter-sized page.

Paragraph two continues the story so the layout engine must place text into
the second column after the first column runs out of vertical space.

Paragraph three, four, and five keep adding volume for a visible two-column
spread with a gutter rule between the bands.

- Item alpha
- Item beta
- Item gamma

Final paragraph confirms list content stayed in-flow.

<!-- columns:1 -->

Single column again.
"#;
        let elements = crate::elements::parse_markdown(md);
        let layout = PageLayout::portrait().with_columns(1); // switched via directive
        let bytes = generate_pdf_bytes(&elements, "Helvetica", 11.0, layout).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        let v = crate::pdf::validate_pdf_bytes(&bytes);
        assert!(v.valid, "{:?}", v.errors);
        assert!(v.page_count >= 1);
    }

    #[test]
    fn test_page_layout_column_geometry() {
        let layout = PageLayout::portrait().with_columns(2).with_column_gap(20.0);
        assert_eq!(layout.column_count(), 2);
        let w = layout.column_width();
        assert!((w * 2.0 + 20.0 - layout.full_content_width()).abs() < 0.01);
        assert!((layout.column_left(0) - layout.margin_left).abs() < 0.01);
        assert!((layout.column_left(1) - (layout.margin_left + w + 20.0)).abs() < 0.01);
    }

    #[test]
    fn test_code_block_with_cjk_uses_unicode_font_and_extracts() {
        if prepare_unicode_font_support().is_none() {
            return;
        }
        let elements = vec![
            Element::Heading {
                level: 1,
                text: "CJK Code".into(),
            },
            Element::Paragraph {
                text: "你好 / こんにちは / 안녕하세요".into(),
            },
            Element::CodeBlock {
                language: "rust".into(),
                code: "// 中文注释\nfn hi() { println!(\"こんにちは\"); }\n// 한국어".into(),
            },
        ];
        let bytes = generate_pdf_bytes_internal(
            &elements,
            "Helvetica",
            11.0,
            PageLayout::portrait(),
            None,
            true,
            None,
        )
        .unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        let raw = String::from_utf8_lossy(&bytes);
        assert!(raw.contains("/ToUnicode"));
        // Write + extract
        let path = std::env::temp_dir().join("pdfrs_cjk_code.pdf");
        std::fs::write(&path, &bytes).unwrap();
        let extracted = crate::pdf::extract_text(path.to_str().unwrap()).unwrap();
        assert!(extracted.contains("你好"), "got: {}", extracted);
        assert!(extracted.contains("こんにちは"), "got: {}", extracted);
        assert!(
            extracted.contains("한국어")
                || extracted.contains("중文")
                || extracted.contains("中文"),
            "got: {}",
            extracted
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_unicode_font_encoder_emits_glyph_ids_not_utf16() {
        let Some((bytes, encoder)) = prepare_unicode_font_support() else {
            return;
        };
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        let gid = face.glyph_index('你').unwrap().0;

        let encoded = encoder.encode_text_as_glyph_ids("你");
        let expected = format!("<{:04X}>", gid);

        assert_eq!(encoded, expected);
        assert_ne!(
            encoded, "<4F60>",
            "must not use unicode code point as CID directly"
        );
    }
}

#[cfg(test)]
mod page_range_tests {
    use super::*;
    use crate::elements::Element;

    #[test]
    fn test_render_page_range_extracts_subset() {
        let elements = vec![
            Element::Paragraph {
                text: "First page content".into(),
            },
            Element::PageBreak,
            Element::Paragraph {
                text: "Second page content".into(),
            },
            Element::PageBreak,
            Element::Paragraph {
                text: "Third page content".into(),
            },
        ];
        let layout = PageLayout::portrait();

        // Extract pages 1..3 (second and third pages, 0-indexed)
        let bytes = render_page_range(&elements, "Helvetica", 12.0, layout, 1..3).unwrap();
        assert!(
            !bytes.is_empty(),
            "Rendered page range should produce non-empty PDF"
        );

        // Verify it's a valid PDF
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.starts_with("%PDF-"), "Should be a valid PDF header");

        // Extract text and verify only pages 2 and 3 are present
        let doc = crate::pdf::PdfDocument::load_from_bytes(&bytes).unwrap();
        let text = doc.get_text().unwrap();
        assert!(
            text.contains("Second page content"),
            "Extracted PDF should contain second page text: {}",
            text
        );
        assert!(
            text.contains("Third page content"),
            "Extracted PDF should contain third page text: {}",
            text
        );
        assert!(
            !text.contains("First page content"),
            "Extracted PDF should NOT contain first page text: {}",
            text
        );
    }

    #[test]
    fn test_render_page_range_single_page() {
        let elements = vec![Element::Paragraph {
            text: "Only page".into(),
        }];
        let layout = PageLayout::portrait();

        let bytes = render_page_range(&elements, "Helvetica", 12.0, layout, 0..1).unwrap();
        let doc = crate::pdf::PdfDocument::load_from_bytes(&bytes).unwrap();
        let text = doc.get_text().unwrap();
        assert!(
            text.contains("Only page"),
            "Single page extraction should work: {}",
            text
        );
    }

    #[test]
    fn test_render_page_range_out_of_bounds() {
        let elements = vec![Element::Paragraph {
            text: "One page".into(),
        }];
        let layout = PageLayout::portrait();

        let result = render_page_range(&elements, "Helvetica", 12.0, layout, 5..10);
        assert!(
            result.is_err(),
            "Out-of-bounds range should return an error"
        );
    }

    #[test]
    fn test_generate_tagged_pdf_bytes() {
        use crate::pdf::{validate_pdf_bytes, validate_pdf_ua_bytes};

        let elements = vec![
            Element::Heading {
                level: 1,
                text: "Tagged Document".into(),
            },
            Element::Paragraph {
                text: "This is an accessible PDF.".into(),
            },
        ];
        let layout = PageLayout::portrait();
        let opts = AccessibilityOptions::new()
            .with_tagged_pdf(true)
            .with_language("en-US".to_string())
            .with_title("Test Tagged PDF".to_string());

        let bytes = generate_tagged_pdf_bytes(&elements, "Helvetica", 12.0, layout, opts).unwrap();
        assert!(!bytes.is_empty(), "Should generate non-empty PDF bytes");

        let content = String::from_utf8_lossy(&bytes);

        // Should contain tagged PDF markers
        assert!(content.contains("/MarkInfo"), "Should contain /MarkInfo");
        assert!(
            content.contains("/Marked true"),
            "Should contain /Marked true"
        );
        assert!(
            content.contains("/StructTreeRoot"),
            "Should contain /StructTreeRoot"
        );
        assert!(content.contains("/Lang"), "Should contain /Lang");
        assert!(content.contains("en-US"), "Should contain language");
        assert!(content.contains("Test Tagged PDF"), "Should contain title");

        // Should be structurally valid
        let validation = validate_pdf_bytes(&bytes);
        assert!(
            validation.valid,
            "Tagged PDF should be structurally valid: {:?}",
            validation.errors
        );

        // Should pass PDF/UA structural checks
        let ua = validate_pdf_ua_bytes(&bytes);
        assert!(ua.has_mark_info, "Should have MarkInfo");
        assert!(ua.has_struct_tree, "Should have StructTreeRoot");
        assert!(ua.has_lang, "Should have Lang");
        assert!(ua.has_title, "Should have Title");
        assert!(
            ua.compliant,
            "Tagged PDF should be PDF/UA compliant: {:?}",
            ua.errors
        );
    }

    #[test]
    fn test_generate_tagged_pdf_bytes_disabled() {
        let elements = vec![Element::Paragraph {
            text: "Untagged".into(),
        }];
        let layout = PageLayout::portrait();
        let opts = AccessibilityOptions::new().with_tagged_pdf(false);

        let bytes = generate_tagged_pdf_bytes(&elements, "Helvetica", 12.0, layout, opts).unwrap();
        let content = String::from_utf8_lossy(&bytes);

        // When tagged_pdf is false, should NOT contain tagged markers
        assert!(
            !content.contains("/MarkInfo"),
            "Should not contain /MarkInfo when disabled"
        );
        assert!(
            !content.contains("/StructTreeRoot"),
            "Should not contain /StructTreeRoot when disabled"
        );
    }

    #[test]
    fn test_markdown_image_and_chart_embedding() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let md = "\
# Media\n\n\
![Sample](sample.png)\n\n\
```chart bar\n\
title: Demo\n\
A, 10\n\
B, 20\n\
```\n";
        let elements = crate::elements::parse_markdown(md);
        assert!(elements.iter().any(|e| matches!(e, Element::Image { .. })));
        assert!(elements.iter().any(|e| matches!(e, Element::Chart { .. })));

        let bytes = generate_pdf_bytes_internal_with_base(
            &elements,
            "Helvetica",
            12.0,
            PageLayout::portrait(),
            None,
            false,
            None,
            Some(fixture),
        )
        .unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        let raw = String::from_utf8_lossy(&bytes);
        assert!(raw.contains("/XObject"), "missing image XObject resource");
        assert!(raw.contains("/Subtype /Image") || raw.contains("/Subtype/Image"));
        assert!(raw.contains("/Im1"), "missing Im1 name");
        let validation = crate::pdf::validate_pdf_bytes(&bytes);
        assert!(validation.valid, "{:?}", validation.errors);
    }

    #[test]
    fn test_thesis_toc_citations_and_folios() {
        let md = "\
<!-- pagenumber:roman -->\n\
<!-- toc -->\n\
<!-- pagebreak -->\n\
# Chapter One\n\n\
Body cites [@alpha] and [@beta].\n\n\
<!-- pagebreak -->\n\
<!-- pagenumber:arabic -->\n\
# Chapter Two\n\n\
More text.\n\n\
<!-- bibliography -->\n\
[@alpha]: Alpha, A. (2020). First.\n\
[@beta]: Beta, B. (2021). Second.\n";
        let elements = crate::elements::parse_markdown(md);
        let bytes =
            generate_pdf_bytes(&elements, "Helvetica", 11.0, PageLayout::portrait()).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        let validation = crate::pdf::validate_pdf_bytes(&bytes);
        assert!(validation.valid, "{:?}", validation.errors);
        assert!(validation.page_count >= 2);

        let tmp = std::env::temp_dir().join("pdfrs_thesis_test.pdf");
        std::fs::write(&tmp, &bytes).unwrap();
        let extracted = crate::pdf::extract_text(tmp.to_str().unwrap()).unwrap();
        assert!(
            extracted.contains("Contents"),
            "missing TOC heading: {extracted}"
        );
        assert!(extracted.contains("Chapter One"), "{extracted}");
        assert!(extracted.contains("Bibliography"), "{extracted}");
        assert!(
            extracted.contains("[1]") && extracted.contains("[2]"),
            "missing citation markers: {extracted}"
        );
    }
}
