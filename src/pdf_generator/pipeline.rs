//! The document generation pipeline: elements in, PDF bytes out.
//!
//! [`generate_pdf_bytes_internal`] is the shared workhorse; the public
//! entry points differ only in which optional features they enable
//! (compression, accessibility tagging, image base directory).

use std::path::PathBuf;

use super::AccessibilityOptions;
use super::assembly::assemble_pdf_bytes;
use super::content_stream::{
    ContentStreamBuilder, OutlineDest, prepare_elements_for_render, render_elements_to_builder,
};
use super::layout::{PageLayout, collect_unicode_chars, document_requires_unicode};
use super::unicode_support::prepare_unicode_font_support_with_subsetting;
use crate::elements::Element;
use crate::error::{PdfError, Result};

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
#[allow(clippy::too_many_arguments)]
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
        return Err(PdfError::Image(format!(
            "Image embedding failed:\n  - {}",
            builder.image_errors.join("\n  - ")
        )));
    }
    let (page_streams, outlines, images) = builder.finish();
    let unicode_arg = match (&unicode_font_support, &used_chars) {
        (Some((bytes, encoder)), Some(chars)) => Some((bytes.as_slice(), encoder, chars)),
        _ => None,
    };
    assemble_pdf_bytes(
        &page_streams,
        font,
        &layout,
        unicode_arg,
        compression_level,
        accessibility,
        &outlines,
        &images,
    )
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
        return Err(PdfError::PageNotFound(range.start));
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
    assemble_pdf_bytes(
        selected,
        font,
        &layout,
        unicode_arg,
        None,
        None,
        &filtered,
        &images,
    )
}
