//! Page layout, color, text alignment, and font-size helpers.

use crate::elements::{Element, TextSegment};

use super::math_layout::{MathPiece, parse_display_math, pieces_to_plain_text};
use super::render_math_text;

// --- Page orientation and layout ---

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageOrientation {
    Portrait,
    Landscape,
}

/// PDF specification version used when generating output.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PdfVersion {
    /// PDF 1.4 (widest compatibility)
    #[default]
    V1_4,
    /// PDF 2.0 (UTF-8 strings, larger object numbers, modern feature set)
    V2_0,
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

pub(super) fn text_requires_unicode(text: &str) -> bool {
    !text.is_ascii()
}

pub(super) fn document_requires_unicode(elements: &[Element]) -> bool {
    elements.iter().any(|elem| match elem {
        Element::Heading { text, .. }
        | Element::Paragraph { text }
        | Element::UnorderedListItem { text, .. }
        | Element::OrderedListItem { text, .. }
        | Element::TaskListItem { text, .. }
        | Element::BlockQuote { text, .. }
        | Element::InlineCode { code: text }
        | Element::StyledText { text, .. } => text_requires_unicode(text),
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
        Element::Link { text, url } => text_requires_unicode(text) || text_requires_unicode(url),
        Element::Image { alt, path } => text_requires_unicode(alt) || text_requires_unicode(path),
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
            | TextSegment::Strikethrough(t) => text_requires_unicode(t),
            TextSegment::MathInline(expr) => text_requires_unicode(&render_math_text(expr)),
            TextSegment::Link { text, url } => {
                text_requires_unicode(text) || text_requires_unicode(url)
            }
            TextSegment::Citation { key } => text_requires_unicode(key),
        }),
        Element::HorizontalRule
        | Element::EmptyLine
        | Element::PageBreak
        | Element::Columns { .. }
        | Element::PageNumberMode { .. }
        | Element::RunningHeaderMode { .. }
        | Element::Toc
        | Element::Bibliography
        | Element::CitationDef { .. } => false,
    })
}

/// Collect all unique characters from elements that would be rendered via the Unicode font encoder.
pub(super) fn collect_unicode_chars(elements: &[Element]) -> std::collections::BTreeSet<char> {
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
            | Element::StyledText { text, .. } => {
                chars.extend(text.chars());
            }
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
            Element::CodeBlock { code, .. } => {
                chars.extend(code.chars());
            }
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
                        | TextSegment::Strikethrough(t) => {
                            chars.extend(t.chars());
                        }
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
            Element::HorizontalRule
            | Element::EmptyLine
            | Element::PageBreak
            | Element::Columns { .. }
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
pub(super) fn heading_font_size(level: u8, base: f32) -> f32 {
    match level {
        1 => base * 2.0,
        2 => base * 1.6,
        3 => base * 1.3,
        4 => base * 1.1,
        5 => base * 1.0,
        _ => base * 0.9,
    }
}

pub(super) fn line_height(font_size: f32) -> f32 {
    font_size + 4.0
}

pub(super) fn is_wide_unicode(ch: char) -> bool {
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

pub(super) fn estimated_text_width(text: &str, font_size: f32, monospace: bool) -> f32 {
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

pub(super) fn split_long_word_for_wrap(word: &str, max_units: usize) -> Vec<String> {
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

// --- Color and text alignment ---

/// RGB color for text rendering (0.0-1.0 per channel)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub fn black() -> Self {
        Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        }
    }
    pub fn red() -> Self {
        Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        }
    }
    pub fn blue() -> Self {
        Color {
            r: 0.0,
            g: 0.0,
            b: 1.0,
        }
    }
    pub fn gray() -> Self {
        Color {
            r: 0.5,
            g: 0.5,
            b: 0.5,
        }
    }
    pub fn rgb(r: f32, g: f32, b: f32) -> Self {
        Color { r, g, b }
    }
}

/// Text alignment for line rendering
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}
