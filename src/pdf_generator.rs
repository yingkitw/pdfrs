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

use crate::elements::{ChartKind, Element, PageNumberStyle, TextSegment};
use crate::image::{self, ImageInfo};
use crate::pdf_ops::escape_pdf_meta;
use crate::table_renderer::{PdfTableHelper, TableStyle};
use crate::thesis::{
    build_bibliography_elements, collect_citation_defs, expand_toc, format_folio, CitationRegistry,
};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[path = "pdf_generator/code_highlight.rs"]
mod code_highlight;
#[path = "pdf_generator/math_layout.rs"]
mod math_layout;
#[path = "pdf_generator/text_support.rs"]
mod text_support;
#[path = "pdf_generator/unicode_support.rs"]
mod unicode_support;
#[path = "pdf_generator/accessibility.rs"]
mod accessibility;

use code_highlight::highlight_code;
use math_layout::{
    line_height_for_pieces, parse_display_math, piece_width, pieces_to_plain_text, MathPiece,
};
use text_support::{encode_pdf_text, use_base14_normalization};
pub(crate) use text_support::{escape_pdf_string, render_math_text};
use unicode_support::{prepare_unicode_font_support_with_subsetting, UnicodeFontEncoder};
#[cfg(test)]
use unicode_support::prepare_unicode_font_support;

// --- Page orientation and layout ---

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageOrientation {
    Portrait,
    Landscape,
}

/// PDF specification version used when generating output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PdfVersion {
    /// PDF 1.4 (widest compatibility)
    V1_4,
    /// PDF 2.0 (UTF-8 strings, larger object numbers, modern feature set)
    V2_0,
}

impl Default for PdfVersion {
    fn default() -> Self {
        PdfVersion::V1_4
    }
}

impl PdfVersion {
    /// Header line emitted at the start of the PDF file.
    pub fn header(&self) -> &'static [u8] {
        match self {
            PdfVersion::V1_4 => b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n",
            PdfVersion::V2_0 => b"%PDF-2.0\n%\xE2\xE3\xCF\xD3\n",
        }
    }

    /// Whether UTF-8 strings (with BOM) are allowed for text-string values.
    pub fn supports_utf8_strings(&self) -> bool {
        matches!(self, PdfVersion::V2_0)
    }
}

fn text_requires_unicode(text: &str) -> bool {
    !text.is_ascii()
}

fn document_requires_unicode(elements: &[Element]) -> bool {
    elements.iter().any(|elem| match elem {
        Element::Heading { text, .. }
        | Element::Paragraph { text }
        | Element::UnorderedListItem { text, .. }
        | Element::OrderedListItem { text, .. }
        | Element::TaskListItem { text, .. }
        | Element::BlockQuote { text, .. }
        | Element::InlineCode { code: text }
        | Element::StyledText { text, .. }
        => text_requires_unicode(text),
        // Math often starts as ASCII LaTeX and may render to unicode symbols.
        // Enable unicode path only when rendered output actually needs it.
        Element::MathBlock { expression } | Element::MathInline { expression } => {
            text_requires_unicode(&render_math_text(expression))
        }
        Element::CodeBlock { code, .. } => text_requires_unicode(code),
        Element::DefinitionItem { term, definition } => {
            text_requires_unicode(term) || text_requires_unicode(definition)
        }
        Element::Footnote { label, text } => {
            text_requires_unicode(label) || text_requires_unicode(text)
        }
        Element::Link { text, url } => {
            text_requires_unicode(text) || text_requires_unicode(url)
        }
        Element::Image { alt, path } => {
            text_requires_unicode(alt) || text_requires_unicode(path)
        }
        Element::Chart { title, points, .. } => {
            title.as_ref().is_some_and(|t| text_requires_unicode(t))
                || points.iter().any(|(l, _)| text_requires_unicode(l))
        }
        Element::TableRow { cells, .. } => cells.iter().any(|c| text_requires_unicode(c)),
        Element::RichParagraph { segments } => segments.iter().any(|seg| match seg {
            TextSegment::Plain(t)
            | TextSegment::Bold(t)
            | TextSegment::Italic(t)
            | TextSegment::BoldItalic(t)
            | TextSegment::Code(t)
            | TextSegment::Strikethrough(t)
            => text_requires_unicode(t),
            TextSegment::MathInline(expr) => text_requires_unicode(&render_math_text(expr)),
            TextSegment::Link { text, url } => {
                text_requires_unicode(text) || text_requires_unicode(url)
            }
            TextSegment::Citation { key } => text_requires_unicode(key),
        }),
        Element::HorizontalRule | Element::EmptyLine | Element::PageBreak | Element::Columns { .. }
        | Element::PageNumberMode { .. }
        | Element::RunningHeaderMode { .. }
        | Element::Toc
        | Element::Bibliography
        | Element::CitationDef { .. } => false,
    })
}

/// Collect all unique characters from elements that would be rendered via the Unicode font encoder.
fn collect_unicode_chars(elements: &[Element]) -> std::collections::BTreeSet<char> {
    let mut chars = std::collections::BTreeSet::new();
    for elem in elements {
        match elem {
            Element::Heading { text, .. }
            | Element::Paragraph { text }
            | Element::UnorderedListItem { text, .. }
            | Element::OrderedListItem { text, .. }
            | Element::TaskListItem { text, .. }
            | Element::BlockQuote { text, .. }
            | Element::InlineCode { code: text }
            | Element::StyledText { text, .. }
            => { chars.extend(text.chars()); }
            Element::MathBlock { expression } | Element::MathInline { expression } => {
                chars.extend(render_math_text(expression).chars());
                let pieces = parse_display_math(expression);
                chars.extend(pieces_to_plain_text(&pieces).chars());
                for piece in pieces {
                    match piece {
                        MathPiece::Text(t) => chars.extend(t.chars()),
                        MathPiece::Fraction {
                            numerator,
                            denominator,
                        } => {
                            chars.extend(numerator.chars());
                            chars.extend(denominator.chars());
                        }
                        MathPiece::Operator {
                            symbol,
                            lower,
                            upper,
                            ..
                        } => {
                            chars.insert(symbol);
                            chars.extend(lower.chars());
                            chars.extend(upper.chars());
                        }
                    }
                }
            }
            Element::CodeBlock { code, .. } => { chars.extend(code.chars()); }
            Element::DefinitionItem { term, definition } => {
                chars.extend(term.chars());
                chars.extend(definition.chars());
            }
            Element::Footnote { label, text } => {
                chars.extend(label.chars());
                chars.extend(text.chars());
            }
            Element::Link { text, url } => {
                chars.extend(text.chars());
                chars.extend(url.chars());
            }
            Element::Image { alt, path } => {
                chars.extend(alt.chars());
                chars.extend(path.chars());
            }
            Element::Chart { title, points, .. } => {
                if let Some(t) = title {
                    chars.extend(t.chars());
                }
                for (label, _) in points {
                    chars.extend(label.chars());
                }
            }
            Element::TableRow { cells, .. } => {
                for cell in cells {
                    chars.extend(cell.chars());
                }
            }
            Element::RichParagraph { segments } => {
                for seg in segments {
                    match seg {
                        TextSegment::Plain(t)
                        | TextSegment::Bold(t)
                        | TextSegment::Italic(t)
                        | TextSegment::BoldItalic(t)
                        | TextSegment::Code(t)
                        | TextSegment::Strikethrough(t)
                        => { chars.extend(t.chars()); }
                        TextSegment::MathInline(expr) => {
                            chars.extend(render_math_text(expr).chars());
                        }
                        TextSegment::Link { text, url } => {
                            chars.extend(text.chars());
                            chars.extend(url.chars());
                        }
                        TextSegment::Citation { key } => {
                            chars.extend(key.chars());
                        }
                    }
                }
            }
            Element::HorizontalRule | Element::EmptyLine | Element::PageBreak | Element::Columns { .. }
            | Element::PageNumberMode { .. }
            | Element::RunningHeaderMode { .. }
            | Element::Toc
            | Element::Bibliography
            | Element::CitationDef { .. } => {}
        }
    }
    chars
}

#[derive(Debug, Clone, Copy)]
pub struct PageLayout {
    pub width: f32,
    pub height: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub version: PdfVersion,
    /// Force right-to-left layout (right-align + visual reorder for RTL scripts).
    /// When false, RTL is still applied automatically for RTL-dominant lines.
    pub rtl: bool,
    /// Number of text columns (1 = single-column). Clamped to 1..=4 when used.
    pub columns: u8,
    /// Gap between columns in points.
    pub column_gap: f32,
}

impl PageLayout {
    pub fn portrait() -> Self {
        PageLayout {
            width: 612.0,
            height: 792.0,
            margin_left: 72.0,
            margin_right: 72.0,
            margin_top: 72.0,
            margin_bottom: 72.0,
            version: PdfVersion::V1_4,
            rtl: false,
            columns: 1,
            column_gap: 18.0,
        }
    }

    pub fn landscape() -> Self {
        PageLayout {
            width: 792.0,
            height: 612.0,
            margin_left: 72.0,
            margin_right: 72.0,
            margin_top: 72.0,
            margin_bottom: 72.0,
            version: PdfVersion::V1_4,
            rtl: false,
            columns: 1,
            column_gap: 18.0,
        }
    }

    pub fn from_orientation(orientation: PageOrientation) -> Self {
        match orientation {
            PageOrientation::Portrait => Self::portrait(),
            PageOrientation::Landscape => Self::landscape(),
        }
    }

    /// Set the PDF version for this document.
    pub fn with_version(mut self, version: PdfVersion) -> Self {
        self.version = version;
        self
    }

    /// Enable right-to-left layout for the document.
    pub fn with_rtl(mut self, rtl: bool) -> Self {
        self.rtl = rtl;
        self
    }

    /// Set multi-column layout (`columns` clamped to 1..=4).
    pub fn with_columns(mut self, columns: u8) -> Self {
        self.columns = columns.clamp(1, 4);
        self
    }

    /// Set the gap between columns (points).
    pub fn with_column_gap(mut self, gap: f32) -> Self {
        self.column_gap = gap.max(0.0);
        self
    }

    pub fn content_top(&self) -> f32 {
        self.height - self.margin_top
    }

    /// Full content width between page margins (all columns + gaps).
    pub fn full_content_width(&self) -> f32 {
        self.width - self.margin_left - self.margin_right
    }

    /// Effective column count (at least 1).
    pub fn column_count(&self) -> u8 {
        self.columns.max(1)
    }

    /// Width of a single text column.
    pub fn column_width(&self) -> f32 {
        let n = self.column_count() as f32;
        let gaps = (n - 1.0).max(0.0) * self.column_gap;
        ((self.full_content_width() - gaps) / n).max(1.0)
    }

    /// Left edge of column `index` (0-based).
    pub fn column_left(&self, index: u8) -> f32 {
        let idx = index.min(self.column_count().saturating_sub(1)) as f32;
        self.margin_left + idx * (self.column_width() + self.column_gap)
    }

    /// Alias for single-column compatibility: width of the active flow column.
    pub fn content_width(&self) -> f32 {
        self.column_width()
    }
}

// --- Font size helpers ---
fn heading_font_size(level: u8, base: f32) -> f32 {
    match level {
        1 => base * 2.0,
        2 => base * 1.6,
        3 => base * 1.3,
        4 => base * 1.1,
        5 => base * 1.0,
        _ => base * 0.9,
    }
}

fn line_height(font_size: f32) -> f32 {
    font_size + 4.0
}

fn is_wide_unicode(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1100..=0x115F
            | 0x2329..=0x232A
            | 0x2E80..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE10..=0xFE19
            | 0xFE30..=0xFE6F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1FAFF
    )
}

fn estimated_text_width(text: &str, font_size: f32, monospace: bool) -> f32 {
    let base = if monospace { 0.6 } else { 0.5 };
    let bytes = text.as_bytes();
    let mut units: f32 = 0.0;
    let mut i = 0;

    while i < bytes.len() {
        // SIMD-like fast path: process 8-byte chunks of pure ASCII
        if i + 8 <= bytes.len() {
            let chunk = &bytes[i..i + 8];
            // Check all 8 bytes are ASCII (high bit clear) with unrolled comparisons.
            // The compiler can vectorise these 8 independent checks into a single SIMD op.
            if chunk[0] < 128
                && chunk[1] < 128
                && chunk[2] < 128
                && chunk[3] < 128
                && chunk[4] < 128
                && chunk[5] < 128
                && chunk[6] < 128
                && chunk[7] < 128
            {
                units += 8.0;
                i += 8;
                continue;
            }
        }

        // Scalar fast path: run of ASCII bytes shorter than a full chunk
        if bytes[i] < 128 {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] < 128 {
                i += 1;
            }
            units += (i - start) as f32;
            continue;
        }

        // Slow path: multi-byte UTF-8 character
        let ch = text[i..].chars().next().unwrap();
        if is_wide_unicode(ch) {
            units += 2.0;
        } else {
            units += 1.3;
        }
        i += ch.len_utf8();
    }

    units * font_size * base
}

fn split_long_word_for_wrap(word: &str, max_units: usize) -> Vec<String> {
    if max_units == 0 {
        return vec![word.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_units = 0usize;

    for ch in word.chars() {
        let ch_units = if ch.is_ascii() {
            1usize
        } else if is_wide_unicode(ch) {
            2usize
        } else {
            1usize
        };

        if !current.is_empty() && current_units + ch_units > max_units {
            chunks.push(current);
            current = String::new();
            current_units = 0;
        }

        current.push(ch);
        current_units += ch_units;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    if chunks.is_empty() {
        vec![word.to_string()]
    } else {
        chunks
    }
}

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
                && let Some(data) = &obj.stream_data {
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

// --- Content stream builder (handles cursor, page breaks, font switches) ---

/// RGB color for text rendering (0.0-1.0 per channel)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub fn black() -> Self { Color { r: 0.0, g: 0.0, b: 0.0 } }
    pub fn red() -> Self { Color { r: 1.0, g: 0.0, b: 0.0 } }
    pub fn blue() -> Self { Color { r: 0.0, g: 0.0, b: 1.0 } }
    pub fn gray() -> Self { Color { r: 0.5, g: 0.5, b: 0.5 } }
    pub fn rgb(r: f32, g: f32, b: f32) -> Self { Color { r, g, b } }
}

/// Text alignment for line rendering
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

struct ContentStreamBuilder {
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
    current_font: String,  // Font name (e.g., "Helvetica", "Helvetica-Bold")
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
    citation_defs: HashMap<String, String>,
    /// Image load/embed failures collected during layout (fail generation if non-empty).
    image_errors: Vec<String>,
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
const FONT_HELVETICA: &str = "Helvetica";
const FONT_HELVETICA_BOLD: &str = "Helvetica-Bold";
const FONT_HELVETICA_OBLIQUE: &str = "Helvetica-Oblique";
const FONT_HELVETICA_BOLD_OBLIQUE: &str = "Helvetica-BoldOblique";
const FONT_COURIER: &str = "Courier";  // Monospace for code

impl ContentStreamBuilder {
    fn new(
        base_font_size: f32,
        show_page_numbers: bool,
        layout: PageLayout,
        unicode_font_encoder: Option<UnicodeFontEncoder>,
        image_base_dir: Option<PathBuf>,
    ) -> Self {
        let mut b = ContentStreamBuilder {
            pages: Vec::new(),
            current: Vec::new(),
            y: layout.content_top(),
            base_font_size,
            current_font_size: base_font_size,
            current_color: Color::black(),
            page_number: 1,
            show_page_numbers,
            layout,
            current_font: FONT_HELVETICA.to_string(),
            current_font_bold: false,
            current_font_italic: false,
            unicode_font_encoder,
            outlines: Vec::new(),
            current_column: 0,
            content_placed_on_page: false,
            column_top_y: layout.content_top(),
            images: Vec::new(),
            image_base_dir,
            page_num_style: PageNumberStyle::Arabic,
            folio: 1,
            running_header_enabled: false,
            running_header_text: String::new(),
            figure_counter: 0,
            table_counter: 0,
            in_abstract: false,
            citations: CitationRegistry::new(),
            citation_defs: HashMap::new(),
            image_errors: Vec::new(),
        };
        b.begin_page();
        b
    }

    fn content_left(&self) -> f32 {
        self.layout.column_left(self.current_column)
    }

    fn content_width(&self) -> f32 {
        self.layout.column_width()
    }

    fn mark_content_placed(&mut self) {
        self.content_placed_on_page = true;
    }

    fn begin_page(&mut self) {
        self.current.clear();
        self.current_column = 0;
        self.content_placed_on_page = false;
        self.y = self.layout.content_top();
        self.column_top_y = self.y;
        self.current.extend_from_slice(b"BT\n");
        self.set_font_with_style(self.base_font_size, false, false);
        self.draw_column_gutters();
        self.draw_running_header();
    }

    fn draw_running_header(&mut self) {
        if !self.running_header_enabled || self.running_header_text.is_empty() {
            return;
        }
        let header = self.running_header_text.clone();
        let size = 9.0;
        let y = self.layout.height - self.layout.margin_top / 2.0;
        let x = self.layout.margin_left;
        // Draw outside the main text object briefly.
        self.current.extend_from_slice(b"ET\nBT\n");
        self.current
            .extend_from_slice(format!("/{} {} Tf\n", FONT_HELVETICA, size).as_bytes());
        self.current
            .extend_from_slice(format!("0.35 0.35 0.35 rg\n").as_bytes());
        self.current
            .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, y).as_bytes());
        self.current.extend_from_slice(
            format!("{} Tj\n", self.encode_text_for_current_font(&header)).as_bytes(),
        );
        // Thin rule under header
        self.current.extend_from_slice(b"ET\n");
        let x2 = self.layout.margin_left + self.layout.full_content_width();
        self.current.extend_from_slice(b"0.75 0.75 0.75 RG\n0.4 w\n");
        self.current.extend_from_slice(
            format!(
                "{:.2} {:.2} m {:.2} {:.2} l S\n",
                self.layout.margin_left,
                y - 4.0,
                x2,
                y - 4.0
            )
            .as_bytes(),
        );
        self.current.extend_from_slice(b"BT\n");
        self.set_font_with_style(self.base_font_size, false, false);
        self.reset_color();
    }

    /// Light vertical rules between columns (drawn in the page content stream).
    fn draw_column_gutters(&mut self) {
        let n = self.layout.column_count();
        if n <= 1 {
            return;
        }
        let top = self.layout.content_top();
        let bottom = self.layout.margin_bottom;
        let color = Color::rgb(0.82, 0.82, 0.82);
        // Gutters are drawn outside the text object.
        self.current.extend_from_slice(b"ET\n");
        for i in 1..n {
            let x = self.layout.column_left(i) - self.layout.column_gap / 2.0;
            self.current.extend_from_slice(
                format!("{} {} {} RG\n", color.r, color.g, color.b).as_bytes(),
            );
            self.current.extend_from_slice(b"0.4 w\n");
            self.current.extend_from_slice(
                format!("{} {} m {} {} l S\n", x, bottom, x, top).as_bytes(),
            );
        }
        self.current.extend_from_slice(b"BT\n");
        self.set_font_with_style(self.base_font_size, false, false);
    }

    /// Advance to the next column, or to a new page when the last column is full.
    fn advance_column_or_page(&mut self) {
        let n = self.layout.column_count();
        if self.current_column + 1 < n {
            self.current_column += 1;
            self.y = self.column_top_y;
        } else {
            self.new_page();
        }
    }

    /// Ensure `extra` vertical space is available in the current column.
    fn ensure_space(&mut self, extra: f32) {
        if self.y - extra < self.layout.margin_bottom {
            self.advance_column_or_page();
        }
    }

    /// Switch column count mid-document. Starts a fresh page only if real
    /// content was already placed on the current page.
    fn set_columns(&mut self, columns: u8) {
        let columns = columns.clamp(1, 4);
        if self.layout.columns == columns {
            return;
        }
        let placed = self.content_placed_on_page || self.current_column > 0;
        self.layout.columns = columns;
        if placed {
            self.new_page();
        } else {
            self.current_column = 0;
            self.current.clear();
            self.begin_page();
        }
    }

    fn set_font(&mut self, size: f32) {
        self.set_font_with_style(size, self.current_font_bold, self.current_font_italic);
    }

    fn set_font_with_style(&mut self, size: f32, bold: bool, italic: bool) {
        self.current_font_size = size;
        self.current_font_bold = bold;
        self.current_font_italic = italic;

        let font_name = match (bold, italic) {
            (true, true) => FONT_HELVETICA_BOLD_OBLIQUE,
            (true, false) => FONT_HELVETICA_BOLD,
            (false, true) => FONT_HELVETICA_OBLIQUE,
            (false, false) => FONT_HELVETICA,
        };

        if self.current_font != font_name {
            self.current_font = font_name.to_string();
        }

        // Use the current font
        self.current
            .extend_from_slice(format!("/{} {} Tf\n", font_name, size).as_bytes());
    }

    fn set_monospace_font(&mut self, size: f32) {
        self.current_font_size = size;
        // When a Unicode Type0 font is embedded, use it for code too so CJK and
        // other non-Latin glyphs in code/comments render correctly. Courier
        // (Base-14) cannot draw those glyphs.
        if self.unicode_font_encoder.is_some() && !use_base14_normalization() {
            self.current_font = FONT_HELVETICA.to_string();
            self.current_font_bold = false;
            self.current_font_italic = false;
            self.current
                .extend_from_slice(format!("/{} {} Tf\n", FONT_HELVETICA, size).as_bytes());
        } else {
            self.current_font = FONT_COURIER.to_string();
            self.current
                .extend_from_slice(format!("/{} {} Tf\n", FONT_COURIER, size).as_bytes());
        }
    }

    /// Width of code text using the same font path as `set_monospace_font`.
    fn code_text_width(&self, text: &str, font_size: f32) -> f32 {
        if self.unicode_font_encoder.is_some() && !use_base14_normalization() {
            if let Some(enc) = &self.unicode_font_encoder {
                return enc.estimate_width(text, font_size);
            }
        }
        estimated_text_width(text, font_size, true)
    }

    /// Wrap a code line to the content width (space-aware, then hard-wrap).
    fn wrap_code_line(&self, line: &str, max_width: f32, font_size: f32) -> Vec<String> {
        if line.is_empty() {
            return vec![String::new()];
        }
        if self.code_text_width(line, font_size) <= max_width {
            return vec![line.to_string()];
        }

        let mut lines = Vec::new();
        let mut current = String::new();

        // Prefer wrapping at whitespace when possible.
        for word in line.split_inclusive(char::is_whitespace) {
            let test = format!("{}{}", current, word);
            if !current.is_empty() && self.code_text_width(&test, font_size) > max_width {
                lines.push(std::mem::take(&mut current));
                // Hard-wrap an oversized single token.
                if self.code_text_width(word, font_size) > max_width {
                    let mut chunk = String::new();
                    for ch in word.chars() {
                        let next = format!("{}{}", chunk, ch);
                        if !chunk.is_empty() && self.code_text_width(&next, font_size) > max_width {
                            lines.push(std::mem::take(&mut chunk));
                            chunk.push(ch);
                        } else {
                            chunk = next;
                        }
                    }
                    current = chunk;
                } else {
                    current = word.to_string();
                }
            } else {
                current = test;
            }
        }
        if !current.is_empty() || lines.is_empty() {
            lines.push(current);
        }
        lines
    }

    fn draw_rectangle(&mut self, x: f32, y: f32, width: f32, height: f32, fill_color: Color) {
        // End text block temporarily to draw rectangle
        self.current.extend_from_slice(b"ET\n");

        // Set fill color
        self.current.extend_from_slice(
            format!("{} {} {} rg\n", fill_color.r, fill_color.g, fill_color.b).as_bytes()
        );

        // Draw and fill rectangle
        self.current.extend_from_slice(
            format!("{} {} {} {} re f\n", x, y, width, height).as_bytes()
        );

        // Resume text block
        self.current.extend_from_slice(b"BT\n");
        self.set_font(self.current_font_size);
        // Always reset to black text after drawing rectangle
        self.current_color = Color::black();
        self.current.extend_from_slice(
            "0 0 0 rg\n".to_string().as_bytes()
        );
    }

    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, line_width: f32, color: Color) {
        // End text block temporarily to draw line
        self.current.extend_from_slice(b"ET\n");

        // Set stroke color and line width
        self.current.extend_from_slice(
            format!("{} {} {} RG\n", color.r, color.g, color.b).as_bytes()
        );
        self.current.extend_from_slice(
            format!("{} w\n", line_width).as_bytes()
        );

        // Draw line
        self.current.extend_from_slice(
            format!("{} {} m {} {} l S\n", x1, y1, x2, y2).as_bytes()
        );

        // Resume text block
        self.current.extend_from_slice(b"BT\n");
        self.set_font(self.current_font_size);
        // Reset to current text color
        self.current.extend_from_slice(
            format!("{} {} {} rg\n", self.current_color.r, self.current_color.g, self.current_color.b).as_bytes()
        );
    }

    /// Render a complete table with borders, text wrapping, and alignment
    fn render_table(&mut self, rows: &[Vec<String>], base_font_size: f32, alignments: Option<&[crate::elements::TableAlignment]>) {
        if rows.is_empty() {
            return;
        }

        let table_helper = PdfTableHelper::default();
        let style = TableStyle::default();

        // Convert string rows to TableRow with alignments
        let table_rows = table_helper.convert_rows(rows, alignments);

        // Calculate table dimensions
        let dims = table_helper.renderer().calculate_dimensions(
            &table_rows,
            &style,
            base_font_size,
            self.content_width(),
        );

        if dims.num_cols == 0 || dims.num_rows == 0 {
            return;
        }

        let line_h = line_height(base_font_size);
        let approx_char_width = if self.unicode_font_encoder.is_some() && !use_base14_normalization() {
            // Prefer measured average from a typical Latin sample when CID fonts are active.
            self.estimate_text_width("abcdefghijklmnopqrstuvwxyz", base_font_size) / 26.0
        } else {
            base_font_size * 0.5
        };

        // Add margin above table
        self.y -= style.margin_top;

        self.ensure_space(dims.total_height + style.margin_top + style.margin_bottom);
        if self.y < self.layout.content_top() - 1.0 {
            // After column/page advance, re-apply top margin.
            self.y -= style.margin_top;
        }

        let start_x = self.content_left();
        let start_y = self.y;

        // Draw outer border
        self.current.extend_from_slice(b"ET\n");
        let (br, bg, bb) = style.border_color;
        self.current.extend_from_slice(
            format!("{} {} {} RG\n", br, bg, bb).as_bytes()
        );
        self.current.extend_from_slice(
            format!("{} w\n", style.border_width).as_bytes()
        );
        self.current.extend_from_slice(
            format!("{} {} m {} {} l S\n", start_x, start_y, start_x + dims.total_width, start_y).as_bytes()
        );
        self.current.extend_from_slice(
            format!("{} {} m {} {} l S\n", start_x, start_y - dims.total_height, start_x + dims.total_width, start_y - dims.total_height).as_bytes()
        );
        self.current.extend_from_slice(
            format!("{} {} m {} {} l S\n", start_x, start_y, start_x, start_y - dims.total_height).as_bytes()
        );
        self.current.extend_from_slice(
            format!("{} {} m {} {} l S\n", start_x + dims.total_width, start_y, start_x + dims.total_width, start_y - dims.total_height).as_bytes()
        );

        // Draw horizontal grid lines
        let mut current_y = start_y;
        for (i, &row_h) in dims.row_heights.iter().enumerate() {
            if i > 0 {
                let (gr, gg, gb) = style.grid_color;
                self.current.extend_from_slice(
                    format!("{} {} {} RG\n", gr, gg, gb).as_bytes()
                );
                self.current.extend_from_slice(
                    format!("{} w\n", style.grid_line_width).as_bytes()
                );
                self.current.extend_from_slice(
                    format!("{} {} m {} {} l S\n", start_x, current_y, start_x + dims.total_width, current_y).as_bytes()
                );
            }
            current_y -= row_h;
        }

        // Draw vertical grid lines
        let mut current_x = start_x;
        for i in 1..dims.num_cols {
            current_x += dims.column_widths[i - 1];
            let (gr, gg, gb) = style.grid_color;
            self.current.extend_from_slice(
                format!("{} {} {} RG\n", gr, gg, gb).as_bytes()
            );
            self.current.extend_from_slice(
                format!("{} w\n", style.grid_line_width).as_bytes()
            );
            self.current.extend_from_slice(
                format!("{} {} m {} {} l S\n", current_x, start_y, current_x, start_y - dims.total_height).as_bytes()
            );
        }

        // Resume text block
        self.current.extend_from_slice(b"BT\n");
        self.set_font(base_font_size);
        self.current.extend_from_slice(b"0 0 0 rg\n");

        // Draw cell contents with wrapping and alignment
        let mut row_y = start_y;
        for (row_idx, row) in table_rows.iter().enumerate() {
            let mut col_x = start_x;
            for (col_idx, cell) in row.cells.iter().enumerate() {
                if col_idx >= dims.num_cols { break; }
                let cell_width = dims.column_widths[col_idx];
                let cell_height = dims.row_heights[row_idx];
                let max_chars = ((cell_width - style.cell_padding * 2.0) / approx_char_width).floor().max(1.0) as usize;

                // Wrap text into lines using the table helper
                let wrapped = table_helper.renderer().wrap_text(&cell.content, max_chars);

                // Calculate vertical centering
                let text_height = wrapped.line_count as f32 * line_h;
                let start_y_pos = row_y - (cell_height - text_height) / 2.0 - line_h / 3.0;

                // Render each line with proper alignment
                for (line_idx, line) in wrapped.lines.iter().enumerate() {
                    let line_width = self.estimate_text_width(line, base_font_size);

                    // Calculate X position using the table helper
                    let x = table_helper.renderer().calculate_text_x(
                        &cell.alignment,
                        col_x,
                        cell_width,
                        line_width,
                        style.cell_padding,
                    );

                    let y = start_y_pos - (line_idx as f32 * line_h);

                    self.current.extend_from_slice(
                        format!("1 0 0 1 {} {} Tm\n", x, y).as_bytes()
                    );
                    self.current.extend_from_slice(
                        format!("{} Tj\n", self.encode_text_for_current_font(line)).as_bytes()
                    );
                }

                col_x += cell_width;
            }
            row_y -= dims.row_heights[row_idx];
        }

        self.y -= dims.total_height + style.margin_bottom;
        self.table_counter += 1;
        let caption = format!("Table {}.", self.table_counter);
        let caption_size = base_font_size * 0.85;
        self.set_color(Color::gray());
        self.set_font_with_style(caption_size, false, true);
        self.emit_line_aligned(&caption, caption_size, TextAlign::Center);
        self.set_font_with_style(base_font_size, false, false);
        self.reset_color();
        self.emit_empty_line();
        self.mark_content_placed();
    }

    /// Approximate text width for wrapping calculations
    fn estimate_text_width(&self, text: &str, font_size: f32) -> f32 {
        if self.current_font != FONT_COURIER
            && let Some(encoder) = &self.unicode_font_encoder
            && !use_base14_normalization()
        {
            return encoder.estimate_width(text, font_size);
        }
        estimated_text_width(text, font_size, self.current_font == FONT_COURIER)
    }

    /// Emit wrapped text that fits within the content width
    fn emit_wrapped_text(&mut self, text: &str, font_size: f32) {
        let max_width = self.content_width();

        if self.estimate_text_width(text, font_size) <= max_width {
            self.emit_line(text, font_size);
            return;
        }

        let approx_char_width = if self.unicode_font_encoder.is_some() && !use_base14_normalization()
        {
            // Average Latin advance under Identity-H ≈ 0.5em once real `/W` is used.
            font_size * 0.5
        } else {
            font_size * 0.5
        };
        let max_chars = (max_width / approx_char_width).floor().max(1.0) as usize;

        let words: Vec<String> = text
            .split_whitespace()
            .flat_map(|word| {
                if self.estimate_text_width(word, font_size) > max_width {
                    split_long_word_for_wrap(word, max_chars)
                } else {
                    vec![word.to_string()]
                }
            })
            .collect();

        let mut current_line = String::new();

        for word in words {
            let test_line = if current_line.is_empty() {
                word.clone()
            } else {
                format!("{} {}", current_line, word)
            };

            if self.estimate_text_width(&test_line, font_size) <= max_width {
                current_line = test_line;
            } else {
                if !current_line.is_empty() {
                    self.emit_line(&current_line, font_size);
                }
                current_line = word;
            }
        }

        if !current_line.is_empty() {
            self.emit_line(&current_line, font_size);
        }
    }

    /// Emit a rich paragraph with per-segment bold/italic/code fonts and wrapping.
    fn emit_rich_paragraph(&mut self, segments: &[TextSegment], font_size: f32) {
        #[derive(Clone)]
        struct Run {
            text: String,
            bold: bool,
            italic: bool,
            mono: bool,
            strike: bool,
        }

        let mut runs: Vec<Run> = Vec::new();
        for segment in segments {
            match segment {
                TextSegment::Plain(text) => {
                    if !text.is_empty() {
                        runs.push(Run {
                            text: text.clone(),
                            bold: false,
                            italic: false,
                            mono: false,
                            strike: false,
                        });
                    }
                }
                TextSegment::Strikethrough(text) => {
                    if !text.is_empty() {
                        runs.push(Run {
                            text: text.clone(),
                            bold: false,
                            italic: false,
                            mono: false,
                            strike: true,
                        });
                    }
                }
                TextSegment::Bold(text) => runs.push(Run {
                    text: text.clone(),
                    bold: true,
                    italic: false,
                    mono: false,
                    strike: false,
                }),
                TextSegment::Italic(text) => runs.push(Run {
                    text: text.clone(),
                    bold: false,
                    italic: true,
                    mono: false,
                    strike: false,
                }),
                TextSegment::BoldItalic(text) => runs.push(Run {
                    text: text.clone(),
                    bold: true,
                    italic: true,
                    mono: false,
                    strike: false,
                }),
                TextSegment::Code(code) => runs.push(Run {
                    text: code.clone(),
                    bold: false,
                    italic: false,
                    mono: true,
                    strike: false,
                }),
                TextSegment::MathInline(expr) => runs.push(Run {
                    text: render_math_text(expr),
                    bold: false,
                    italic: true,
                    mono: false,
                    strike: false,
                }),
                TextSegment::Link { text, url } => {
                    runs.push(Run {
                        text: text.clone(),
                        bold: false,
                        italic: false,
                        mono: false,
                        strike: false,
                    });
                    runs.push(Run {
                        text: format!(" ({})", url),
                        bold: false,
                        italic: false,
                        mono: false,
                        strike: false,
                    });
                }
                TextSegment::Citation { key } => {
                    let marker = self.citation_marker(key);
                    runs.push(Run {
                        text: marker,
                        bold: false,
                        italic: false,
                        mono: false,
                        strike: false,
                    });
                }
            }
        }

        if runs.is_empty() {
            return;
        }

        #[derive(Clone)]
        struct Token {
            text: String,
            bold: bool,
            italic: bool,
            mono: bool,
            strike: bool,
            space_before: bool,
        }

        let mut tokens: Vec<Token> = Vec::new();
        let mut prev_ended_with_space = false;
        for run in &runs {
            let starts_with_space = run.text.starts_with(char::is_whitespace);
            let ends_with_space = run.text.ends_with(char::is_whitespace);
            let mut first_in_run = true;
            let mut chars = run.text.chars().peekable();
            while chars.peek().is_some() {
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                if word.is_empty() {
                    break;
                }
                let space_before = if first_in_run {
                    !tokens.is_empty() && (starts_with_space || prev_ended_with_space)
                } else {
                    true
                };
                first_in_run = false;
                tokens.push(Token {
                    text: word,
                    bold: run.bold,
                    italic: run.italic,
                    mono: run.mono,
                    strike: run.strike,
                    space_before,
                });
            }
            prev_ended_with_space = ends_with_space;
        }

        let max_width = self.content_width();
        let lh = line_height(font_size);
        let mut line: Vec<Token> = Vec::new();
        let mut line_width = 0.0f32;

        let measure = |builder: &Self, text: &str, mono: bool| -> f32 {
            let size = if mono { font_size * 0.9 } else { font_size };
            if mono {
                estimated_text_width(text, size, true)
            } else {
                builder.estimate_text_width(text, size)
            }
        };

        let flush_line = |builder: &mut Self, line: &mut Vec<Token>| {
            if line.is_empty() {
                return;
            }
            builder.ensure_space(lh);
            let mut x = builder.content_left();
            for (i, tok) in line.iter().enumerate() {
                if i > 0 && tok.space_before {
                    x += measure(builder, " ", false);
                }
                if tok.mono {
                    builder.set_monospace_font(font_size * 0.9);
                } else {
                    builder.set_font_with_style(font_size, tok.bold, tok.italic);
                }
                builder
                    .current
                    .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, builder.y).as_bytes());
                let encoded = builder.encode_text_for_current_font(&tok.text);
                builder
                    .current
                    .extend_from_slice(format!("{} Tj\n", encoded).as_bytes());
                let w = measure(builder, &tok.text, tok.mono);
                if tok.strike {
                    let strike_y = builder.y + font_size * 0.3;
                    builder.draw_line(
                        x,
                        strike_y,
                        x + w,
                        strike_y,
                        0.7,
                        Color::black(),
                    );
                }
                x += w;
            }
            builder.set_font_with_style(font_size, false, false);
            builder.y -= lh;
            line.clear();
        };

        for tok in tokens {
            let space_w = if tok.space_before && !line.is_empty() {
                measure(self, " ", false)
            } else {
                0.0
            };
            let tok_w = measure(self, &tok.text, tok.mono);
            if !line.is_empty() && line_width + space_w + tok_w > max_width {
                flush_line(self, &mut line);
                line.push(Token {
                    space_before: false,
                    ..tok
                });
                line_width = tok_w;
            } else {
                line_width += space_w + tok_w;
                line.push(tok);
            }
        }
        flush_line(self, &mut line);
    }

    /// Render a display-math block with stacked fractions and operator limits.
    fn emit_display_math(&mut self, expression: &str, base_font_size: f32) {
        let math_size = base_font_size * 1.28;
        let padding = 10.0;
        // Flatten multi-line matrix environments first, then split remaining rows.
        let flattened = text_support::flatten_math_environments(expression);
        let lines: Vec<&str> = flattened
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if lines.is_empty() {
            return;
        }

        let parsed: Vec<Vec<MathPiece>> = lines.iter().map(|l| parse_display_math(l)).collect();
        let row_heights: Vec<f32> = parsed
            .iter()
            .map(|pieces| line_height_for_pieces(pieces, math_size))
            .collect();
        let block_height: f32 = row_heights.iter().sum::<f32>() + padding * 2.0;

        self.emit_empty_line();
        self.ensure_space(block_height);

        let bg_color = Color::rgb(0.93, 0.95, 1.0);
        let rect_x = self.content_left() - padding;
        let rect_y = self.y - block_height;
        let rect_width = self.content_width() + padding * 2.0;
        self.draw_rectangle(rect_x, rect_y, rect_width, block_height, bg_color);

        let accent_color = Color::rgb(0.3, 0.4, 0.8);
        self.draw_line(rect_x, rect_y, rect_x, rect_y + block_height, 2.0, accent_color);

        self.set_color(Color::rgb(0.08, 0.1, 0.28));

        let measure = |builder: &Self, text: &str, size: f32| -> f32 {
            if builder.unicode_font_encoder.is_some() && !use_base14_normalization() {
                if let Some(enc) = &builder.unicode_font_encoder {
                    return enc.estimate_width(text, size);
                }
            }
            estimated_text_width(text, size, false)
        };

        let mut cursor_y = self.y - padding;
        for (pieces, row_h) in parsed.iter().zip(row_heights.iter()) {
            let axis_y = cursor_y - row_h * 0.55;
            let total_w: f32 = pieces
                .iter()
                .map(|p| piece_width(p, math_size, &|t, s| measure(self, t, s)))
                .sum();
            let mut x = self.content_left()
                + ((self.content_width() - total_w) / 2.0).max(4.0);

            for piece in pieces {
                match piece {
                    MathPiece::Text(text) => {
                        self.set_font_with_style(math_size, false, true);
                        self.current.extend_from_slice(
                            format!("1 0 0 1 {} {} Tm\n", x, axis_y).as_bytes(),
                        );
                        let enc = self.encode_text_for_current_font(text);
                        self.current
                            .extend_from_slice(format!("{} Tj\n", enc).as_bytes());
                        x += measure(self, text, math_size);
                    }
                    MathPiece::Operator {
                        symbol,
                        lower,
                        upper,
                        side_limits,
                    } => {
                        let op_size = math_size * 1.45;
                        let script = math_size * 0.55;
                        let sym = symbol.to_string();
                        let sym_w = measure(self, &sym, op_size);

                        if *side_limits {
                            // Integral-style: large op, scripts to the right.
                            self.set_font_with_style(op_size, false, false);
                            self.current.extend_from_slice(
                                format!("1 0 0 1 {} {} Tm\n", x, axis_y - op_size * 0.18).as_bytes(),
                            );
                            let enc = self.encode_text_for_current_font(&sym);
                            self.current
                                .extend_from_slice(format!("{} Tj\n", enc).as_bytes());

                            let sx = x + sym_w * 0.72;
                            if !upper.is_empty() {
                                self.set_font_with_style(script, false, false);
                                self.current.extend_from_slice(
                                    format!(
                                        "1 0 0 1 {} {} Tm\n",
                                        sx,
                                        axis_y + script * 1.05
                                    )
                                    .as_bytes(),
                                );
                                let enc = self.encode_text_for_current_font(upper);
                                self.current
                                    .extend_from_slice(format!("{} Tj\n", enc).as_bytes());
                            }
                            if !lower.is_empty() {
                                self.set_font_with_style(script, false, false);
                                self.current.extend_from_slice(
                                    format!(
                                        "1 0 0 1 {} {} Tm\n",
                                        sx,
                                        axis_y - script * 1.15
                                    )
                                    .as_bytes(),
                                );
                                let enc = self.encode_text_for_current_font(lower);
                                self.current
                                    .extend_from_slice(format!("{} Tj\n", enc).as_bytes());
                            }
                            let lim_w = measure(self, lower, script).max(measure(self, upper, script));
                            x += sym_w + 4.0 + lim_w;
                        } else {
                            // Sum/prod: limits above and below, centered on symbol.
                            let lim_w = measure(self, lower, script).max(measure(self, upper, script));
                            let col_w = sym_w.max(lim_w);
                            let cx = x + col_w / 2.0;

                            if !upper.is_empty() {
                                let uw = measure(self, upper, script);
                                self.set_font_with_style(script, false, false);
                                self.current.extend_from_slice(
                                    format!(
                                        "1 0 0 1 {} {} Tm\n",
                                        cx - uw / 2.0,
                                        axis_y + op_size * 0.62
                                    )
                                    .as_bytes(),
                                );
                                let enc = self.encode_text_for_current_font(upper);
                                self.current
                                    .extend_from_slice(format!("{} Tj\n", enc).as_bytes());
                            }

                            self.set_font_with_style(op_size, false, false);
                            self.current.extend_from_slice(
                                format!(
                                    "1 0 0 1 {} {} Tm\n",
                                    cx - sym_w / 2.0,
                                    axis_y - op_size * 0.12
                                )
                                .as_bytes(),
                            );
                            let enc = self.encode_text_for_current_font(&sym);
                            self.current
                                .extend_from_slice(format!("{} Tj\n", enc).as_bytes());

                            if !lower.is_empty() {
                                let lw = measure(self, lower, script);
                                self.set_font_with_style(script, false, false);
                                self.current.extend_from_slice(
                                    format!(
                                        "1 0 0 1 {} {} Tm\n",
                                        cx - lw / 2.0,
                                        axis_y - op_size * 0.78
                                    )
                                    .as_bytes(),
                                );
                                let enc = self.encode_text_for_current_font(lower);
                                self.current
                                    .extend_from_slice(format!("{} Tj\n", enc).as_bytes());
                            }
                            x += col_w + 8.0;
                        }
                    }
                    MathPiece::Fraction {
                        numerator,
                        denominator,
                    } => {
                        let script = math_size * 0.85;
                        let nw = measure(self, numerator, script);
                        let dw = measure(self, denominator, script);
                        let w = nw.max(dw) + 6.0;
                        let cx = x + w / 2.0;

                        self.set_font_with_style(script, false, true);
                        self.current.extend_from_slice(
                            format!(
                                "1 0 0 1 {} {} Tm\n",
                                cx - nw / 2.0,
                                axis_y + script * 0.75
                            )
                            .as_bytes(),
                        );
                        let enc = self.encode_text_for_current_font(numerator);
                        self.current
                            .extend_from_slice(format!("{} Tj\n", enc).as_bytes());

                        // Fraction bar
                        let bar_y = axis_y + script * 0.12;
                        self.draw_line(
                            cx - w / 2.0 + 1.0,
                            bar_y,
                            cx + w / 2.0 - 1.0,
                            bar_y,
                            0.9,
                            Color::rgb(0.08, 0.1, 0.28),
                        );

                        self.set_font_with_style(script, false, true);
                        self.current.extend_from_slice(
                            format!(
                                "1 0 0 1 {} {} Tm\n",
                                cx - dw / 2.0,
                                axis_y - script * 0.95
                            )
                            .as_bytes(),
                        );
                        let enc = self.encode_text_for_current_font(denominator);
                        self.current
                            .extend_from_slice(format!("{} Tj\n", enc).as_bytes());

                        x += w + 2.0;
                    }
                }
            }

            cursor_y -= *row_h;
        }

        self.y -= block_height;
        self.set_font_with_style(base_font_size, false, false);
        self.reset_color();
        self.emit_empty_line();
    }

    fn set_color(&mut self, color: Color) {
        self.current_color = color;
        self.current.extend_from_slice(
            format!("{} {} {} rg\n", color.r, color.g, color.b).as_bytes(),
        );
    }

    fn reset_color(&mut self) {
        self.set_color(Color::black());
    }

    fn end_text_block(&mut self) {
        self.current.extend_from_slice(b"ET\n");
    }

    fn add_page_number(&mut self) {
        let Some(label) = format_folio(self.page_num_style, self.folio) else {
            return;
        };
        let approx = self.estimate_text_width(&label, 9.0);
        let x = self.layout.margin_left + (self.layout.full_content_width() - approx) / 2.0;
        let y = self.layout.margin_bottom / 2.0;
        let encoded_label = if let Some(encoder) = &self.unicode_font_encoder {
            if use_base14_normalization() {
                encode_pdf_text(&label)
            } else {
                encoder.encode_text_as_glyph_ids(&label)
            }
        } else {
            encode_pdf_text(&label)
        };
        self.current.extend_from_slice(b"BT\n");
        self.current
            .extend_from_slice(format!("/{} 9 Tf\n", FONT_HELVETICA).as_bytes());
        self.current
            .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, y).as_bytes());
        self.current
            .extend_from_slice(format!("{} Tj\n", encoded_label).as_bytes());
        self.current.extend_from_slice(b"ET\n");
    }

    fn new_page(&mut self) {
        self.end_text_block();
        if self.show_page_numbers {
            self.add_page_number();
        }
        self.pages.push(std::mem::take(&mut self.current));
        self.page_number += 1;
        if self.page_num_style != PageNumberStyle::None {
            self.folio += 1;
        }
        self.begin_page();
    }

    fn emit_line(&mut self, text: &str, font_size: f32) {
        self.emit_line_aligned(text, font_size, TextAlign::Left);
    }

    fn emit_line_aligned(&mut self, text: &str, font_size: f32, align: TextAlign) {
        let lh = line_height(font_size);
        self.ensure_space(lh);
        self.set_font(font_size);

        let use_rtl = self.layout.rtl || crate::rtl::prefers_rtl_layout(text);
        let display = if use_rtl {
            crate::rtl::prepare_for_pdf(text)
        } else {
            text.to_string()
        };
        let align = if use_rtl && matches!(align, TextAlign::Left) {
            TextAlign::Right
        } else {
            align
        };

        let x = match align {
            TextAlign::Left => self.content_left(),
            TextAlign::Center => {
                let approx_width = self.estimate_text_width(&display, font_size);
                self.content_left() + (self.content_width() - approx_width) / 2.0
            }
            TextAlign::Right => {
                let approx_width = self.estimate_text_width(&display, font_size);
                self.content_left() + self.content_width() - approx_width
            }
            TextAlign::Justify => self.content_left(),
        };

        self.current
            .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, self.y).as_bytes());
        self.current
            .extend_from_slice(format!("{} Tj\n", self.encode_text_for_current_font(&display)).as_bytes());
        self.y -= lh;
        self.mark_content_placed();
    }

    /// Centered heading across the full page width (spans all columns).
    fn emit_full_width_heading(&mut self, text: &str, font_size: f32) {
        let lh = line_height(font_size);
        // Full-width bands always start in column 0 at the shared column top.
        self.current_column = 0;
        self.y = self.y.min(self.column_top_y);
        self.ensure_space(lh);
        self.set_font(font_size);
        let approx_width = self.estimate_text_width(text, font_size);
        let x = self.layout.margin_left
            + (self.layout.full_content_width() - approx_width) / 2.0;
        self.current
            .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, self.y).as_bytes());
        self.current
            .extend_from_slice(format!("{} Tj\n", self.encode_text_for_current_font(text)).as_bytes());
        self.y -= lh;
        self.column_top_y = self.y;
        self.mark_content_placed();
    }

    fn encode_text_for_current_font(&self, text: &str) -> String {
        if self.current_font != FONT_COURIER
            && let Some(encoder) = &self.unicode_font_encoder
                && !use_base14_normalization() {
                    return encoder.encode_text_as_glyph_ids(text);
                }
        encode_pdf_text(text)
    }

    fn emit_empty_line(&mut self) {
        let lh = line_height(self.base_font_size) * 0.5;
        self.ensure_space(lh);
        self.y -= lh;
    }

    fn emit_horizontal_rule(&mut self) {
        // Add spacing above the rule
        self.y -= line_height(self.base_font_size) / 2.0;

        self.ensure_space(line_height(self.base_font_size));

        // Draw a horizontal line across the content area
        let x1 = self.content_left();
        let x2 = self.content_left() + self.content_width();
        let y = self.y;
        let line_width = 1.0;
        let color = Color::gray();

        self.draw_line(x1, y, x2, y, line_width, color);

        // Add spacing below the rule
        self.y -= line_height(self.base_font_size);
    }

    fn finish(mut self) -> (Vec<Vec<u8>>, Vec<OutlineDest>, Vec<(String, ImageInfo)>) {
        self.end_text_block();
        if self.show_page_numbers {
            self.add_page_number();
        }
        self.pages.push(self.current);
        (self.pages, self.outlines, self.images)
    }

    fn resolve_image_path(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            return p.to_path_buf();
        }
        if let Some(base) = &self.image_base_dir {
            let joined = base.join(p);
            return std::fs::canonicalize(&joined).unwrap_or(joined);
        }
        p.to_path_buf()
    }

    /// Embed a raster image (JPEG/PNG/BMP), scaled to the content width.
    fn emit_image(&mut self, alt: &str, path: &str) {
        let resolved = self.resolve_image_path(path);
        let loaded = image::load_image_with_alt_text(
            resolved.to_string_lossy().as_ref(),
            if alt.is_empty() {
                None
            } else {
                Some(alt.to_string())
            },
        );

        let Ok(info) = loaded else {
            self.image_errors.push(format!(
                "failed to load image '{}' ({})",
                alt, resolved.display()
            ));
            return;
        };

        let max_w = self.content_width();
        let max_h = (self.layout.height * 0.45).min(320.0);
        let (dw, dh) = image::scale_to_fit(info.width, info.height, max_w, max_h);
        let caption_h = line_height(self.base_font_size * 0.85);
        let gap = 6.0;
        self.ensure_space(dh + caption_h + gap * 2.0);
        self.y -= gap;

        let name = format!("Im{}", self.images.len() + 1);
        let x = self.content_left() + (self.content_width() - dw) / 2.0;
        let y_bottom = self.y - dh;

        // Paint outside the text object.
        self.current.extend_from_slice(b"ET\n");
        self.current.extend_from_slice(b"q\n");
        self.current.extend_from_slice(
            format!("{:.2} 0 0 {:.2} {:.2} {:.2} cm\n", dw, dh, x, y_bottom).as_bytes(),
        );
        self.current
            .extend_from_slice(format!("/{} Do\n", name).as_bytes());
        self.current.extend_from_slice(b"Q\n");
        self.current.extend_from_slice(b"BT\n");
        self.set_font_with_style(self.base_font_size, false, false);
        self.reset_color();

        self.images.push((name, info));
        self.y = y_bottom - gap;
        self.mark_content_placed();

        self.figure_counter += 1;
        let caption = if alt.is_empty() {
            format!("Figure {}.", self.figure_counter)
        } else {
            format!("Figure {}. {}", self.figure_counter, alt)
        };
        let caption_size = self.base_font_size * 0.85;
        self.set_color(Color::gray());
        self.set_font_with_style(caption_size, false, true);
        self.emit_line_aligned(&caption, caption_size, TextAlign::Center);
        self.set_font_with_style(self.base_font_size, false, false);
        self.reset_color();
        self.emit_empty_line();
    }

    /// Draw a bar / line / pie chart from labeled numeric points.
    fn emit_chart(
        &mut self,
        kind: ChartKind,
        title: &Option<String>,
        points: &[(String, f32)],
    ) {
        if points.is_empty() {
            return;
        }

        let title_h = if title.is_some() {
            line_height(self.base_font_size)
        } else {
            0.0
        };
        let plot_h = match kind {
            ChartKind::Pie => 170.0,
            _ => 150.0,
        };
        let legend_h = if matches!(kind, ChartKind::Pie) {
            (points.len() as f32) * line_height(self.base_font_size * 0.8)
        } else {
            0.0
        };
        let total_h = title_h + plot_h + legend_h + 16.0;
        self.ensure_space(total_h);

        self.figure_counter += 1;
        let fig_title = match title {
            Some(t) => format!("Figure {}. {}", self.figure_counter, t),
            None => format!("Figure {}.", self.figure_counter),
        };
        self.set_font_with_style(self.base_font_size, true, false);
        self.emit_line_aligned(&fig_title, self.base_font_size, TextAlign::Center);
        self.set_font_with_style(self.base_font_size, false, false);

        let left = self.content_left();
        let width = self.content_width();
        let top = self.y;
        let bottom = top - plot_h;

        self.current.extend_from_slice(b"ET\n");

        match kind {
            ChartKind::Bar => self.draw_bar_chart(left, bottom, width, plot_h, points),
            ChartKind::Line => self.draw_line_chart(left, bottom, width, plot_h, points),
            ChartKind::Pie => self.draw_pie_chart(left, bottom, width, plot_h, points),
        }

        self.current.extend_from_slice(b"BT\n");
        self.set_font_with_style(self.base_font_size, false, false);
        self.reset_color();
        self.y = bottom - 8.0;
        self.mark_content_placed();

        if matches!(kind, ChartKind::Pie) {
            let label_size = self.base_font_size * 0.8;
            for (i, (label, value)) in points.iter().enumerate() {
                let (r, g, b) = crate::chart::CHART_COLORS[i % crate::chart::CHART_COLORS.len()];
                self.set_color(Color::rgb(r, g, b));
                self.emit_line(
                    &format!("• {} ({})", label, format_chart_value(*value)),
                    label_size,
                );
            }
            self.reset_color();
        }

        self.emit_empty_line();
    }

    fn draw_bar_chart(
        &mut self,
        left: f32,
        bottom: f32,
        width: f32,
        height: f32,
        points: &[(String, f32)],
    ) {
        let max_v = points
            .iter()
            .map(|(_, v)| v.abs())
            .fold(0.0_f32, f32::max)
            .max(1.0);
        let axis = Color::rgb(0.35, 0.35, 0.35);
        let pad_l = 28.0;
        let pad_b = 22.0;
        let pad_t = 8.0;
        let plot_x = left + pad_l;
        let plot_w = width - pad_l - 4.0;
        let plot_y0 = bottom + pad_b;
        let plot_h = height - pad_b - pad_t;

        // Axes
        self.append_stroke(axis, 0.8);
        self.append_line(plot_x, plot_y0, plot_x + plot_w, plot_y0);
        self.append_line(plot_x, plot_y0, plot_x, plot_y0 + plot_h);

        let n = points.len() as f32;
        let slot = plot_w / n;
        let bar_w = (slot * 0.62).max(4.0);

        for (i, (label, value)) in points.iter().enumerate() {
            let (r, g, b) = crate::chart::CHART_COLORS[i % crate::chart::CHART_COLORS.len()];
            let h = (value.abs() / max_v) * plot_h;
            let x = plot_x + i as f32 * slot + (slot - bar_w) / 2.0;
            let y = plot_y0;
            self.current.extend_from_slice(
                format!("{} {} {} rg\n{:.2} {:.2} {:.2} {:.2} re f\n", r, g, b, x, y, bar_w, h)
                    .as_bytes(),
            );
            // Label under bar (short)
            let short = truncate_label(label, 8);
            self.append_fill_text(
                &short,
                x + bar_w / 2.0 - self.estimate_text_width(&short, 7.0) / 2.0,
                bottom + 6.0,
                7.0,
                axis,
            );
        }
    }

    fn draw_line_chart(
        &mut self,
        left: f32,
        bottom: f32,
        width: f32,
        height: f32,
        points: &[(String, f32)],
    ) {
        let max_v = points
            .iter()
            .map(|(_, v)| *v)
            .fold(f32::NEG_INFINITY, f32::max);
        let min_v = points
            .iter()
            .map(|(_, v)| *v)
            .fold(f32::INFINITY, f32::min);
        let span = (max_v - min_v).abs().max(1.0);
        let axis = Color::rgb(0.35, 0.35, 0.35);
        let pad_l = 28.0;
        let pad_b = 22.0;
        let pad_t = 8.0;
        let plot_x = left + pad_l;
        let plot_w = width - pad_l - 4.0;
        let plot_y0 = bottom + pad_b;
        let plot_h = height - pad_b - pad_t;

        self.append_stroke(axis, 0.8);
        self.append_line(plot_x, plot_y0, plot_x + plot_w, plot_y0);
        self.append_line(plot_x, plot_y0, plot_x, plot_y0 + plot_h);

        let n = points.len().max(1);
        let mut coords = Vec::with_capacity(n);
        for (i, (_, value)) in points.iter().enumerate() {
            let t = if n == 1 {
                0.5
            } else {
                i as f32 / (n - 1) as f32
            };
            let x = plot_x + t * plot_w;
            let y = plot_y0 + ((*value - min_v) / span) * plot_h;
            coords.push((x, y));
        }

        let (r, g, b) = crate::chart::CHART_COLORS[0];
        self.current
            .extend_from_slice(format!("{} {} {} RG\n1.5 w\n", r, g, b).as_bytes());
        if let Some((x0, y0)) = coords.first() {
            self.current
                .extend_from_slice(format!("{:.2} {:.2} m\n", x0, y0).as_bytes());
            for (x, y) in coords.iter().skip(1) {
                self.current
                    .extend_from_slice(format!("{:.2} {:.2} l\n", x, y).as_bytes());
            }
            self.current.extend_from_slice(b"S\n");
        }
        for (x, y) in &coords {
            // Small filled square as a point marker (PDF has no `arc` operator).
            self.current.extend_from_slice(
                format!(
                    "{} {} {} rg\n{:.2} {:.2} 3.5 3.5 re f\n",
                    r,
                    g,
                    b,
                    x - 1.75,
                    y - 1.75
                )
                .as_bytes(),
            );
        }

        for (i, (label, _)) in points.iter().enumerate() {
            let short = truncate_label(label, 8);
            let (x, _) = coords[i];
            self.append_fill_text(
                &short,
                x - self.estimate_text_width(&short, 7.0) / 2.0,
                bottom + 6.0,
                7.0,
                axis,
            );
        }
    }

    fn draw_pie_chart(
        &mut self,
        left: f32,
        bottom: f32,
        width: f32,
        height: f32,
        points: &[(String, f32)],
    ) {
        let total: f32 = points.iter().map(|(_, v)| v.abs()).sum::<f32>().max(1.0);
        let cx = left + width * 0.42;
        let cy = bottom + height * 0.52;
        let radius = (width.min(height) * 0.32).min(70.0);

        let mut angle = 0.0_f32; // radians from +x
        for (i, (_, value)) in points.iter().enumerate() {
            let sweep = (value.abs() / total) * std::f32::consts::TAU;
            if sweep <= 0.0 {
                continue;
            }
            let (r, g, b) = crate::chart::CHART_COLORS[i % crate::chart::CHART_COLORS.len()];
            self.append_pie_slice(cx, cy, radius, angle, angle + sweep, Color::rgb(r, g, b));
            angle += sweep;
        }
    }

    fn append_pie_slice(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        a0: f32,
        a1: f32,
        fill: Color,
    ) {
        // Approximate arc with line segments.
        let steps = ((a1 - a0).abs() / 0.2).ceil().max(2.0) as usize;
        self.current.extend_from_slice(
            format!("{} {} {} rg\n{:.2} {:.2} m\n", fill.r, fill.g, fill.b, cx, cy).as_bytes(),
        );
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let a = a0 + (a1 - a0) * t;
            let x = cx + radius * a.cos();
            let y = cy + radius * a.sin();
            self.current
                .extend_from_slice(format!("{:.2} {:.2} l\n", x, y).as_bytes());
        }
        self.current.extend_from_slice(b"h f\n");
    }

    fn append_stroke(&mut self, color: Color, width: f32) {
        self.current.extend_from_slice(
            format!("{} {} {} RG\n{} w\n", color.r, color.g, color.b, width).as_bytes(),
        );
    }

    fn append_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.current.extend_from_slice(
            format!("{:.2} {:.2} m {:.2} {:.2} l S\n", x1, y1, x2, y2).as_bytes(),
        );
    }

    fn append_fill_text(&mut self, text: &str, x: f32, y: f32, size: f32, color: Color) {
        // Nested BT inside graphics section (we are outside the main BT).
        self.current.extend_from_slice(b"BT\n");
        self.current.extend_from_slice(
            format!(
                "/{} {} Tf\n{} {} {} rg\n1 0 0 1 {:.2} {:.2} Tm\n{} Tj\nET\n",
                FONT_HELVETICA,
                size,
                color.r,
                color.g,
                color.b,
                x,
                y,
                self.encode_text_for_current_font(text)
            )
            .as_bytes(),
        );
    }

    fn push_outline(&mut self, title: &str, level: u8) {
        let page_label = format_folio(self.page_num_style, self.folio).unwrap_or_default();
        self.outlines.push(OutlineDest {
            title: title.to_string(),
            level,
            page_index: (self.page_number as usize).saturating_sub(1),
            y: self.y,
            page_label,
        });
    }

    fn set_page_number_style(&mut self, style: PageNumberStyle) {
        if self.page_num_style == style {
            return;
        }
        self.page_num_style = style;
        if style != PageNumberStyle::None {
            self.folio = 1;
        }
    }

    fn citation_marker(&mut self, key: &str) -> String {
        let n = self.citations.number_for(key);
        format!("[{}]", n)
    }
}

fn truncate_label(label: &str, max_chars: usize) -> String {
    let count = label.chars().count();
    if count <= max_chars {
        return label.to_string();
    }
    label.chars().take(max_chars.saturating_sub(1)).collect::<String>() + "…"
}

fn format_chart_value(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i64)
    } else {
        format!("{:.1}", v)
    }
}

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
    create_pdf_from_elements_with_layout(filename, elements, font, base_font_size, PageLayout::portrait())
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

/// Expand TOC (two-pass outlines) and attach citation definitions for rendering.
fn prepare_elements_for_render(
    elements: &[Element],
    base_font_size: f32,
    layout: PageLayout,
    unicode_font_encoder: Option<UnicodeFontEncoder>,
    image_base_dir: Option<PathBuf>,
) -> (Vec<Element>, HashMap<String, String>) {
    let citation_defs = collect_citation_defs(elements);
    let mut prepared = elements.to_vec();
    if prepared.iter().any(|e| matches!(e, Element::Toc)) {
        let mut dry = ContentStreamBuilder::new(
            base_font_size,
            true,
            layout,
            unicode_font_encoder.clone(),
            image_base_dir.clone(),
        );
        dry.citation_defs = citation_defs.clone();
        render_elements_to_builder(&mut dry, &prepared, base_font_size);
        let (_pages, outlines, _images) = dry.finish();
        prepared = expand_toc(&prepared, &outlines);
    }
    (prepared, citation_defs)
}

/// Render elements into a ContentStreamBuilder (shared by file and bytes APIs)
fn render_elements_to_builder(builder: &mut ContentStreamBuilder, elements: &[Element], base_font_size: f32) {
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut table_alignments: Option<Vec<crate::elements::TableAlignment>> = None;

    for elem in elements {
        // Handle table rows specially - accumulate them
        if let Element::TableRow { cells, is_separator, alignments } = elem {
            if *is_separator {
                // Store alignments from separator row
                table_alignments = Some(alignments.clone());
            } else {
                // Only add non-separator rows to the table
                table_rows.push(cells.clone());
            }
            continue;
        }

        // Flush any accumulated table before rendering non-table element
        if !table_rows.is_empty() {
            builder.render_table(&table_rows, base_font_size, table_alignments.as_deref());
            table_rows.clear();
            table_alignments = None;
        }

        // Render non-table elements
        match elem {
            Element::Heading { level, text } => {
                let fs = heading_font_size(*level, base_font_size);
                builder.emit_empty_line();
                builder.push_outline(text, *level);
                if *level <= 2 {
                    builder.running_header_text = text.clone();
                }
                builder.in_abstract = text.eq_ignore_ascii_case("Abstract");
                builder.set_font_with_style(fs, true, false);
                if *level == 1 && builder.layout.column_count() > 1 {
                    builder.emit_full_width_heading(text, fs);
                } else {
                    let align = if *level == 1 {
                        TextAlign::Center
                    } else {
                        TextAlign::Left
                    };
                    builder.emit_line_aligned(text, fs, align);
                }
                builder.set_font_with_style(base_font_size, false, false);
                builder.emit_empty_line();
                if *level == 1 && builder.layout.column_count() > 1 {
                    builder.column_top_y = builder.y;
                    builder.current_column = 0;
                }
            }
            Element::Paragraph { text } => {
                if builder.in_abstract {
                    builder.set_font_with_style(base_font_size, false, true);
                    builder.emit_wrapped_text(text, base_font_size);
                    builder.set_font_with_style(base_font_size, false, false);
                } else {
                    builder.emit_wrapped_text(text, base_font_size);
                }
            }
            Element::RichParagraph { segments } => {
                builder.emit_rich_paragraph(segments, base_font_size);
            }
            Element::UnorderedListItem { text, depth } => {
                let indent = "  ".repeat(*depth as usize);
                let line = format!("{}- {}", indent, text);
                builder.emit_wrapped_text(&line, base_font_size);
            }
            Element::OrderedListItem { number, text, depth } => {
                let indent = "  ".repeat(*depth as usize);
                let line = format!("{}{}. {}", indent, number, text);
                builder.emit_wrapped_text(&line, base_font_size);
            }
            Element::TaskListItem { checked, text } => {
                let marker = if *checked { "[x]" } else { "[ ]" };
                let line = format!("{} {}", marker, text);
                builder.emit_wrapped_text(&line, base_font_size);
            }
            Element::CodeBlock { code, language } => {
                let code_size = base_font_size * 0.85;
                let padding = 8.0;
                let line_h = line_height(code_size);
                let max_code_width = builder.content_width();
                let all_lines: Vec<&str> = code.lines().collect();

                // Pre-wrap so background height matches rendered lines (incl. CJK).
                let display_lines: Vec<String> = all_lines
                    .iter()
                    .flat_map(|line| builder.wrap_code_line(line, max_code_width, code_size))
                    .collect();

                builder.emit_empty_line();

                let mut line_idx = 0;
                while line_idx < display_lines.len() {
                    let available = builder.y - builder.layout.margin_bottom - padding * 2.0;
                    let max_lines_on_page = (available / line_h).floor().max(1.0) as usize;
                    let chunk_end = (line_idx + max_lines_on_page).min(display_lines.len());
                    let chunk = &display_lines[line_idx..chunk_end];
                    let chunk_height = chunk.len() as f32 * line_h + padding * 2.0;

                    builder.y -= padding;

                    let text_block_height = chunk.len() as f32 * line_h;
                    let bg_color = Color::rgb(0.95, 0.95, 0.95);
                    let rect_x = builder.content_left() - padding;
                    let rect_y = builder.y - text_block_height - padding;
                    let rect_width = builder.content_width() + padding * 2.0;
                    let rect_height = chunk_height;
                    builder.draw_rectangle(rect_x, rect_y, rect_width, rect_height, bg_color);

                    let border_color = Color::rgb(0.75, 0.75, 0.75);
                    builder.draw_line(rect_x, rect_y, rect_x + rect_width, rect_y, 0.5, border_color);
                    builder.draw_line(rect_x, rect_y + rect_height, rect_x + rect_width, rect_y + rect_height, 0.5, border_color);
                    builder.draw_line(rect_x, rect_y, rect_x, rect_y + rect_height, 0.5, border_color);
                    builder.draw_line(rect_x + rect_width, rect_y, rect_x + rect_width, rect_y + rect_height, 0.5, border_color);

                    builder.set_monospace_font(code_size);

                    for code_line in chunk {
                        let line_tokens = highlight_code(code_line, language);

                        if line_tokens.is_empty() || line_tokens.iter().all(|t| t.text.is_empty()) {
                            builder.current.extend_from_slice(
                                format!("{} {} {} rg\n", 0.15, 0.15, 0.15).as_bytes()
                            );
                            builder.current.extend_from_slice(
                                format!("1 0 0 1 {} {} Tm\n", builder.content_left(), builder.y).as_bytes()
                            );
                            builder.current.extend_from_slice(
                                format!("{} Tj\n", builder.encode_text_for_current_font(code_line)).as_bytes()
                            );
                        } else {
                            // Position once, then emit sequential Tj so extractors keep identifiers contiguous.
                            builder.current.extend_from_slice(
                                format!("1 0 0 1 {} {} Tm\n", builder.content_left(), builder.y).as_bytes()
                            );
                            for token in &line_tokens {
                                if token.text.is_empty() {
                                    continue;
                                }
                                builder.current.extend_from_slice(
                                    format!("{} {} {} rg\n", token.color.r, token.color.g, token.color.b)
                                        .as_bytes(),
                                );
                                builder.current.extend_from_slice(
                                    format!("{} Tj\n", builder.encode_text_for_current_font(&token.text))
                                        .as_bytes(),
                                );
                            }
                        }
                        builder.y -= line_h;
                    }

                    builder.y -= padding;
                    line_idx = chunk_end;

                    if line_idx < display_lines.len() {
                        builder.set_font_with_style(base_font_size, false, false);
                        builder.reset_color();
                        builder.new_page();
                    }
                }

                builder.set_font_with_style(base_font_size, false, false);
                builder.reset_color();
                builder.emit_empty_line();
            }
            Element::DefinitionItem { term, definition } => {
                builder.set_font_with_style(base_font_size, true, false);
                builder.emit_wrapped_text(term, base_font_size);
                builder.set_font_with_style(base_font_size, false, false);
                builder.emit_wrapped_text(&format!("  {}", definition), base_font_size);
            }
            Element::InlineCode { code } => {
                let code_size = base_font_size * 0.9;
                builder.set_monospace_font(code_size);
                builder.set_color(Color::gray());
                builder.emit_line(code, code_size);
                builder.set_font_with_style(base_font_size, false, false);
                builder.reset_color();
            }
            Element::Link { text, url } => {
                builder.set_color(Color::blue());
                builder.emit_wrapped_text(&format!("{} ({})", text, url), base_font_size);
                builder.reset_color();
            }
            Element::Image { alt, path } => {
                builder.emit_image(alt, path);
            }
            Element::Chart { kind, title, points } => {
                builder.emit_chart(*kind, title, points);
            }
            Element::StyledText { text, bold, italic } => {
                builder.set_font_with_style(base_font_size, *bold, *italic);
                builder.emit_wrapped_text(text, base_font_size);
                builder.set_font_with_style(base_font_size, false, false);
            }
            Element::PageBreak => {
                builder.new_page();
            }
            Element::Footnote { label, text } => {
                let footnote_size = base_font_size * 0.85;
                builder.emit_wrapped_text(&format!("[{}] {}", label, text), footnote_size);
            }
            Element::BlockQuote { text, depth } => {
                let prefix = "> ".repeat(*depth as usize);
                builder.set_color(Color::gray());
                builder.emit_wrapped_text(&format!("{}{}", prefix, text), base_font_size);
                builder.reset_color();
            }
            Element::MathBlock { expression } => {
                builder.emit_display_math(expression, base_font_size);
            }
            Element::MathInline { expression } => {
                // Render inline math in italic with slight color
                let rendered = render_math_text(expression);
                builder.set_font_with_style(base_font_size, false, true);
                builder.set_color(Color::rgb(0.1, 0.1, 0.3));
                builder.emit_line(&rendered, base_font_size);
                builder.set_font_with_style(base_font_size, false, false);
                builder.reset_color();
            }
            Element::HorizontalRule => {
                builder.emit_horizontal_rule();
            }
            Element::EmptyLine => {
                builder.emit_empty_line();
            }
            Element::Columns { count } => {
                builder.set_columns(*count);
            }
            Element::PageNumberMode { style } => {
                builder.set_page_number_style(*style);
            }
            Element::RunningHeaderMode { enabled } => {
                builder.running_header_enabled = *enabled;
            }
            Element::Toc => {
                // Expanded in prepare_elements_for_render when present.
            }
            Element::Bibliography => {
                let defs = builder.citation_defs.clone();
                let bib = build_bibliography_elements(&builder.citations, &defs);
                // Render inline without re-entering the table flusher.
                for b in &bib {
                    match b {
                        Element::Heading { level, text } => {
                            let fs = heading_font_size(*level, base_font_size);
                            builder.emit_empty_line();
                            builder.push_outline(text, *level);
                            builder.set_font_with_style(fs, true, false);
                            builder.emit_line_aligned(text, fs, TextAlign::Left);
                            builder.set_font_with_style(base_font_size, false, false);
                            builder.emit_empty_line();
                        }
                        Element::Paragraph { text } => {
                            builder.emit_wrapped_text(text, base_font_size);
                        }
                        Element::EmptyLine => builder.emit_empty_line(),
                        _ => {}
                    }
                }
            }
            Element::CitationDef { .. } => {
                // Collected up-front; not rendered inline.
            }
            Element::TableRow { .. } => {
                // Already handled above
            }
        }
    }

    // Flush any remaining table
    if !table_rows.is_empty() {
        builder.render_table(&table_rows, base_font_size, table_alignments.as_deref());
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
    unicode_font: Option<(&[u8], &UnicodeFontEncoder, &std::collections::BTreeSet<char>)>,
) -> FontResourceIds {
    let helvetica_id = if let Some((bytes, encoder, chars)) = unicode_font {
        let font_file_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", bytes.len()),
            bytes.to_vec(),
        );

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
        let tounicode_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", tounicode.len()),
            tounicode,
        );

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
    generate_pdf_bytes_internal(elements, font, base_font_size, layout, compression_level, false, None)
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
    generate_pdf_bytes_internal(elements, font, base_font_size, layout, None, false, Some(&options))
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
    unicode_font: Option<(&[u8], &UnicodeFontEncoder, &std::collections::BTreeSet<char>)>,
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
                Ok(compressed) if compressed.len() < page_stream.len() => {
                    (format!("<< /Length {} /Filter /FlateDecode >>\n", compressed.len()), compressed)
                }
                _ => (format!("<< /Length {} >>\n", page_stream.len()), page_stream.clone()),
            }
        } else {
            (format!("<< /Length {} >>\n", page_stream.len()), page_stream.clone())
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
            FONT_HELVETICA, font_ids.helvetica,
            FONT_HELVETICA_BOLD, font_ids.helvetica_bold,
            FONT_HELVETICA_OBLIQUE, font_ids.helvetica_oblique,
            FONT_HELVETICA_BOLD_OBLIQUE, font_ids.helvetica_bold_oblique,
            FONT_COURIER, font_ids.courier,
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
        && opts.tagged_pdf {
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



pub use accessibility::{AccessibilityOptions, StructureElement, StructureType, element_to_structure};


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
        assert_eq!(elem.actual_text, Some("This is the actual text".to_string()));
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
        let elem = Element::Heading { level: 1, text: "Hello".into() };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::H1);
        assert_eq!(struct_elem.actual_text, Some("Hello".to_string()));
    }

    #[test]
    fn test_element_to_structure_paragraph() {
        let elem = Element::Paragraph { text: "Test paragraph".into() };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::P);
        assert_eq!(struct_elem.actual_text, Some("Test paragraph".to_string()));
    }

    #[test]
    fn test_element_to_structure_code() {
        let elem = Element::CodeBlock { language: "rust".into(), code: "fn main() {}".into() };
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

        let gaussian = render_math_text(
            r"\exp\left(-\frac{(x - \mu)^2}{2\sigma^2}\right)",
        );
        assert!(
            !gaussian.contains("≤ft") && !gaussian.contains("\\left"),
            "\\le must not eat \\left: {}",
            gaussian
        );
        assert!(gaussian.contains("exp"), "rendered: {}", gaussian);
    }

    #[test]
    fn test_render_math_text_matrices() {
        let m = render_math_text(
            r"\begin{bmatrix} a & b \\ c & d \end{bmatrix}",
        );
        assert!(m.contains('[') && m.contains(']'), "rendered: {}", m);
        assert!(m.contains('a') && m.contains('d'), "rendered: {}", m);
        assert!(!m.contains("begin"), "rendered: {}", m);

        let v = render_math_text(
            r"\begin{vmatrix} \hat{i} & \hat{j} \\ a_1 & a_2 \end{vmatrix}",
        );
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
        assert!(rendered.contains("∫₀¹") || rendered.contains("∫[0→1]"), "rendered: {}", rendered);
        assert!(rendered.contains("∑ᵢⁿ") || rendered.contains("∑[i→n]"), "rendered: {}", rendered);
        assert!(rendered.contains("x²") || rendered.contains("x^(2)"), "rendered: {}", rendered);
        assert!(rendered.contains("aᵢ") || rendered.contains("a_(i)"), "rendered: {}", rendered);
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
        assert!(expr1.contains("∫₀¹"), "Should render integral with subscript/superscript: {}", expr1);
        assert!(expr1.contains("∑"), "Should contain sum symbol: {}", expr1);
        assert!(
            expr1.contains("∑[i=1→n]") || expr1.contains("∑ᵢ"),
            "Sum limits should be readable: {}",
            expr1
        );
        assert!(expr1.contains("x²"), "Should render x squared: {}", expr1);
        
        let expr2 = render_math_text(r"\prod_{k=1}^{m} b_k");
        assert!(expr2.contains("∏"), "Should contain product symbol: {}", expr2);
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
        
        let expr3 = render_math_text(r"\forall x \in \mathbb{R}, x \geq 0 \Rightarrow \sqrt{x} \in \mathbb{R}");
        assert!(expr3.contains("∀"), "Should contain forall: {}", expr3);
        assert!(expr3.contains("∈"), "Should contain element of: {}", expr3);
        assert!(expr3.contains("ℝ"), "Should contain real numbers: {}", expr3);
        assert!(expr3.contains("≥"), "Should contain greater or equal: {}", expr3);
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
        let bytes =
            generate_pdf_bytes_internal(&elements, "Helvetica", 11.0, PageLayout::portrait(), None, true, None)
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
        assert!(extracted.contains("한국어") || extracted.contains("중文") || extracted.contains("中文"), "got: {}", extracted);
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
        assert_ne!(encoded, "<4F60>", "must not use unicode code point as CID directly");
    }

    #[test]
    fn test_math_oblique_path_uses_unicode_glyph_encoding() {
        let Some((_bytes, encoder)) = prepare_unicode_font_support() else {
            return;
        };

        let mut builder = ContentStreamBuilder::new(
            12.0,
            false,
            PageLayout::portrait(),
            Some(encoder.clone()),
            None,
        );
        builder.set_font_with_style(12.0, false, true); // math path uses oblique

        let encoded = builder.encode_text_for_current_font("∑∞≈");
        let expected = encoder.encode_text_as_glyph_ids("∑∞≈");

        assert_eq!(encoded, expected);
    }

    #[test]
    fn test_ascii_text_uses_glyph_ids_when_unicode_font_mode_active() {
        if use_base14_normalization() {
            return;
        }
        let Some((_bytes, encoder)) = prepare_unicode_font_support() else {
            return;
        };

        let mut builder = ContentStreamBuilder::new(
            12.0,
            false,
            PageLayout::portrait(),
            Some(encoder.clone()),
            None,
        );
        builder.set_font_with_style(12.0, true, false);

        let encoded = builder.encode_text_for_current_font("Unicode Test");
        let expected = encoder.encode_text_as_glyph_ids("Unicode Test");

        assert_eq!(encoded, expected);
    }
}

#[cfg(test)]
mod page_range_tests {
    use super::*;
    use crate::elements::Element;

    #[test]
    fn test_render_page_range_extracts_subset() {
        let elements = vec![
            Element::Paragraph { text: "First page content".into() },
            Element::PageBreak,
            Element::Paragraph { text: "Second page content".into() },
            Element::PageBreak,
            Element::Paragraph { text: "Third page content".into() },
        ];
        let layout = PageLayout::portrait();

        // Extract pages 1..3 (second and third pages, 0-indexed)
        let bytes = render_page_range(&elements, "Helvetica", 12.0, layout, 1..3).unwrap();
        assert!(!bytes.is_empty(), "Rendered page range should produce non-empty PDF");

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
        let elements = vec![
            Element::Paragraph { text: "Only page".into() },
        ];
        let layout = PageLayout::portrait();

        let bytes = render_page_range(&elements, "Helvetica", 12.0, layout, 0..1).unwrap();
        let doc = crate::pdf::PdfDocument::load_from_bytes(&bytes).unwrap();
        let text = doc.get_text().unwrap();
        assert!(text.contains("Only page"), "Single page extraction should work: {}", text);
    }

    #[test]
    fn test_render_page_range_out_of_bounds() {
        let elements = vec![
            Element::Paragraph { text: "One page".into() },
        ];
        let layout = PageLayout::portrait();

        let result = render_page_range(&elements, "Helvetica", 12.0, layout, 5..10);
        assert!(result.is_err(), "Out-of-bounds range should return an error");
    }

    #[test]
    fn test_generate_tagged_pdf_bytes() {
        use crate::pdf::{validate_pdf_ua_bytes, validate_pdf_bytes};

        let elements = vec![
            Element::Heading { level: 1, text: "Tagged Document".into() },
            Element::Paragraph { text: "This is an accessible PDF.".into() },
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
        assert!(content.contains("/Marked true"), "Should contain /Marked true");
        assert!(content.contains("/StructTreeRoot"), "Should contain /StructTreeRoot");
        assert!(content.contains("/Lang"), "Should contain /Lang");
        assert!(content.contains("en-US"), "Should contain language");
        assert!(content.contains("Test Tagged PDF"), "Should contain title");

        // Should be structurally valid
        let validation = validate_pdf_bytes(&bytes);
        assert!(validation.valid, "Tagged PDF should be structurally valid: {:?}", validation.errors);

        // Should pass PDF/UA structural checks
        let ua = validate_pdf_ua_bytes(&bytes);
        assert!(ua.has_mark_info, "Should have MarkInfo");
        assert!(ua.has_struct_tree, "Should have StructTreeRoot");
        assert!(ua.has_lang, "Should have Lang");
        assert!(ua.has_title, "Should have Title");
        assert!(ua.compliant, "Tagged PDF should be PDF/UA compliant: {:?}", ua.errors);
    }

    #[test]
    fn test_generate_tagged_pdf_bytes_disabled() {
        let elements = vec![
            Element::Paragraph { text: "Untagged".into() },
        ];
        let layout = PageLayout::portrait();
        let opts = AccessibilityOptions::new().with_tagged_pdf(false);

        let bytes = generate_tagged_pdf_bytes(&elements, "Helvetica", 12.0, layout, opts).unwrap();
        let content = String::from_utf8_lossy(&bytes);

        // When tagged_pdf is false, should NOT contain tagged markers
        assert!(!content.contains("/MarkInfo"), "Should not contain /MarkInfo when disabled");
        assert!(!content.contains("/StructTreeRoot"), "Should not contain /StructTreeRoot when disabled");
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
        let bytes = generate_pdf_bytes(&elements, "Helvetica", 11.0, PageLayout::portrait()).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        let validation = crate::pdf::validate_pdf_bytes(&bytes);
        assert!(validation.valid, "{:?}", validation.errors);
        assert!(validation.page_count >= 2);

        let tmp = std::env::temp_dir().join("pdfrs_thesis_test.pdf");
        std::fs::write(&tmp, &bytes).unwrap();
        let extracted = crate::pdf::extract_text(tmp.to_str().unwrap()).unwrap();
        assert!(extracted.contains("Contents"), "missing TOC heading: {extracted}");
        assert!(extracted.contains("Chapter One"), "{extracted}");
        assert!(extracted.contains("Bibliography"), "{extracted}");
        assert!(
            extracted.contains("[1]") && extracted.contains("[2]"),
            "missing citation markers: {extracted}"
        );
    }
}
