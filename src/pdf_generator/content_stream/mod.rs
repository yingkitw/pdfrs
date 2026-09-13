//! Content stream builder: cursor management, page breaks, font switches,
//! and element-to-stream rendering.

mod builder;
mod charts;
mod elements;
mod math;
mod page_assembly;
mod render;

use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::Result;

use crate::elements::{Element, PageNumberStyle};
use crate::image::ImageInfo;
use crate::thesis::CitationRegistry;

use super::generate_pdf_bytes_internal_with_base;
use super::layout::{Color, PageLayout};
use super::unicode_support::UnicodeFontEncoder;

pub(crate) use render::{prepare_elements_for_render, render_elements_to_builder};

pub(crate) struct ContentStreamBuilder {
    pages: Vec<Vec<u8>>,
    current: Vec<u8>,
    y: f32,
    base_font_size: f32,
    current_font_size: f32,
    current_color: Color,
    page_number: u32,
    show_page_numbers: bool,
    layout: PageLayout,
    // Font state
    current_font: String, // Font name (e.g., "Helvetica", "Helvetica-Bold")
    current_font_bold: bool,
    current_font_italic: bool,
    unicode_font_encoder: Option<UnicodeFontEncoder>,
    /// Bookmark destinations collected while rendering headings.
    outlines: Vec<OutlineDest>,
    /// Current column index (0-based) when `layout.columns > 1`.
    current_column: u8,
    /// True once non-spacer content has been painted on the current page.
    content_placed_on_page: bool,
    /// Y position where columns restart after a full-width band (e.g. H1).
    column_top_y: f32,
    /// Images referenced from content streams (`/ImN Do`), created at assemble time.
    images: Vec<(String, ImageInfo)>,
    /// Directory used to resolve relative markdown image paths.
    image_base_dir: Option<PathBuf>,
    /// Displayed folio style (arabic / roman / none).
    page_num_style: PageNumberStyle,
    /// 1-based folio counter (restarts when style switches to roman/arabic).
    folio: u32,
    /// Running header enabled.
    running_header_enabled: bool,
    /// Current running header text (typically chapter / section title).
    running_header_text: String,
    /// Auto-number figures (images + charts).
    figure_counter: u32,
    /// Auto-number tables.
    table_counter: u32,
    /// Italic abstract body until the next heading.
    in_abstract: bool,
    /// Citation numbering registry.
    citations: CitationRegistry,
    /// Citation key → full reference text.
    pub(crate) citation_defs: HashMap<String, String>,
    /// Image load/embed failures collected during layout (fail generation if non-empty).
    pub(crate) image_errors: Vec<String>,
}

/// A PDF outline (bookmark) destination produced during layout.
#[derive(Debug, Clone)]
pub struct OutlineDest {
    pub title: String,
    pub level: u8,
    /// Zero-based page index.
    pub page_index: usize,
    pub y: f32,
    /// Displayed folio label at outline time (roman or arabic).
    pub page_label: String,
}

// Font name constants
pub(crate) const FONT_HELVETICA: &str = "Helvetica";
pub(crate) const FONT_HELVETICA_BOLD: &str = "Helvetica-Bold";
pub(crate) const FONT_HELVETICA_OBLIQUE: &str = "Helvetica-Oblique";
pub(crate) const FONT_HELVETICA_BOLD_OBLIQUE: &str = "Helvetica-BoldOblique";
pub(crate) const FONT_COURIER: &str = "Courier"; // Monospace for code

// --- Public API ---

pub fn create_pdf(filename: &str, text: &str) -> Result<()> {
    create_pdf_with_options(filename, text, "Helvetica", 12.0)
}

/// Legacy plain-text pipeline (backward compatible)
pub fn create_pdf_with_options(
    filename: &str,
    text: &str,
    font: &str,
    font_size: f32,
) -> Result<()> {
    let elements: Vec<Element> = text
        .lines()
        .map(|l| {
            if l.trim().is_empty() {
                Element::EmptyLine
            } else {
                Element::Paragraph {
                    text: l.to_string(),
                }
            }
        })
        .collect();
    create_pdf_from_elements(filename, &elements, font, font_size)
}

/// Rich element-based pipeline with header sizes, page numbers, etc.
pub fn create_pdf_from_elements(
    filename: &str,
    elements: &[Element],
    font: &str,
    base_font_size: f32,
) -> Result<()> {
    create_pdf_from_elements_with_layout(
        filename,
        elements,
        font,
        base_font_size,
        PageLayout::portrait(),
    )
}

/// Rich element-based pipeline with configurable page layout (orientation)
pub fn create_pdf_from_elements_with_layout(
    filename: &str,
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
) -> Result<()> {
    let bytes = generate_pdf_bytes_internal_with_base(
        elements,
        font,
        base_font_size,
        layout,
        None,
        false,
        None,
        None,
    )?;
    std::fs::write(filename, bytes)?;
    Ok(())
}

/// Same as `create_pdf_from_elements_with_layout` but with optional stream compression
pub fn create_pdf_from_elements_with_layout_and_compression(
    filename: &str,
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
    compression_level: Option<u8>,
) -> Result<()> {
    let bytes = generate_pdf_bytes_internal_with_base(
        elements,
        font,
        base_font_size,
        layout,
        compression_level,
        false,
        None,
        None,
    )?;
    std::fs::write(filename, bytes)?;
    Ok(())
}
