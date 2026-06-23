use crate::elements::{Element, TextSegment};
use crate::pdf_ops::escape_pdf_meta;
use crate::table_renderer::{PdfTableHelper, TableStyle};
use anyhow::Result;
use std::fs::File;
use std::io::Write;

#[path = "pdf_generator/code_highlight.rs"]
mod code_highlight;
#[path = "pdf_generator/text_support.rs"]
mod text_support;
#[path = "pdf_generator/unicode_support.rs"]
mod unicode_support;

use code_highlight::highlight_code;
use text_support::{encode_pdf_text, escape_pdf_string, render_math_text, use_base14_normalization};
use unicode_support::{prepare_unicode_font_support, UnicodeFontEncoder};

// --- Page orientation and layout ---

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageOrientation {
    Portrait,
    Landscape,
}

fn text_requires_unicode(text: &str) -> bool {
    text.chars().any(|ch| !ch.is_ascii())
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
        Element::TableRow { cells, .. } => cells.iter().any(|c| text_requires_unicode(c)),
        Element::RichParagraph { segments } => segments.iter().any(|seg| match seg {
            TextSegment::Plain(t)
            | TextSegment::Bold(t)
            | TextSegment::Italic(t)
            | TextSegment::BoldItalic(t)
            | TextSegment::Code(t)
            => text_requires_unicode(t),
            TextSegment::MathInline(expr) => text_requires_unicode(&render_math_text(expr)),
            TextSegment::Link { text, url } => {
                text_requires_unicode(text) || text_requires_unicode(url)
            }
        }),
        Element::HorizontalRule | Element::EmptyLine | Element::PageBreak => false,
    })
}

#[derive(Debug, Clone, Copy)]
pub struct PageLayout {
    pub width: f32,
    pub height: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
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
        }
    }

    pub fn from_orientation(orientation: PageOrientation) -> Self {
        match orientation {
            PageOrientation::Portrait => Self::portrait(),
            PageOrientation::Landscape => Self::landscape(),
        }
    }

    pub fn content_top(&self) -> f32 {
        self.height - self.margin_top
    }

    pub fn content_width(&self) -> f32 {
        self.width - self.margin_left - self.margin_right
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
    let units: f32 = text
        .chars()
        .map(|ch| {
            if ch.is_ascii() {
                1.0
            } else if is_wide_unicode(ch) {
                2.0
            } else {
                1.3
            }
        })
        .sum();
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
}

#[derive(Debug)]
pub struct PdfObj {
    pub id: u32,
    pub generation: u32,
    pub content: String,
    pub is_stream: bool,
    pub stream_data: Option<Vec<u8>>,
}

impl PdfGenerator {
    pub fn new() -> Self {
        PdfGenerator {
            objects: Vec::new(),
            next_id: 1,
            info_id: None,
        }
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
        pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

        // Calculate offsets for xref table
        let mut offsets = Vec::new();
        let mut current_offset = pdf.len() as u32;

        // Write objects and collect offsets
        for obj in &self.objects {
            offsets.push(current_offset);
            let obj_header = format!("{} {} obj\n", obj.id, obj.generation);
            pdf.extend_from_slice(obj_header.as_bytes());
            pdf.extend_from_slice(obj.content.as_bytes());

            if obj.is_stream {
                if let Some(data) = &obj.stream_data {
                    pdf.extend_from_slice(b"stream\n");
                    pdf.extend_from_slice(data);
                    pdf.extend_from_slice(b"\nendstream\n");
                }
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
    total_pages: u32,
    show_page_numbers: bool,
    layout: PageLayout,
    // Font state
    current_font: String,  // Font name (e.g., "Helvetica", "Helvetica-Bold")
    current_font_bold: bool,
    current_font_italic: bool,
    unicode_font_encoder: Option<UnicodeFontEncoder>,
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
    ) -> Self {
        let mut b = ContentStreamBuilder {
            pages: Vec::new(),
            current: Vec::new(),
            y: layout.content_top(),
            base_font_size,
            current_font_size: base_font_size,
            current_color: Color::black(),
            page_number: 1,
            total_pages: 0,
            show_page_numbers,
            layout,
            current_font: FONT_HELVETICA.to_string(),
            current_font_bold: false,
            current_font_italic: false,
            unicode_font_encoder,
        };
        b.begin_page();
        b
    }

    fn begin_page(&mut self) {
        self.current.clear();
        self.y = self.layout.content_top();
        self.current.extend_from_slice(b"BT\n");
        self.set_font_with_style(self.base_font_size, false, false);
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
        self.current_font = FONT_COURIER.to_string();
        self.current
            .extend_from_slice(format!("/{} {} Tf\n", FONT_COURIER, size).as_bytes());
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
            format!("0 0 0 rg\n").as_bytes()
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
            self.layout.content_width(),
        );

        if dims.num_cols == 0 || dims.num_rows == 0 {
            return;
        }

        let line_h = line_height(base_font_size);
        let approx_char_width = base_font_size * 0.5;

        // Add margin above table
        self.y -= style.margin_top;

        // Check for page break
        if self.needs_page_break(dims.total_height + style.margin_top + style.margin_bottom) {
            self.new_page();
            self.y -= style.margin_top;
        }

        let start_x = self.layout.margin_left;
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
                    let line_width = line.len() as f32 * approx_char_width;

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
    }

    /// Approximate text width for wrapping calculations
    fn estimate_text_width(&self, text: &str, font_size: f32) -> f32 {
        estimated_text_width(text, font_size, self.current_font == FONT_COURIER)
    }

    /// Emit wrapped text that fits within the content width
    fn emit_wrapped_text(&mut self, text: &str, font_size: f32) {
        let max_width = self.layout.content_width();
        let approx_char_width = font_size * 0.5;
        let max_chars = (max_width / approx_char_width).floor().max(1.0) as usize;

        if text.chars().count() <= max_chars {
            self.emit_line(text, font_size);
            return;
        }

        let words: Vec<String> = text
            .split_whitespace()
            .flat_map(|word| {
                if word.chars().count() > max_chars {
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

    fn set_color(&mut self, color: Color) {
        self.current_color = color;
        self.current.extend_from_slice(
            format!("{} {} {} rg\n", color.r, color.g, color.b).as_bytes(),
        );
    }

    fn reset_color(&mut self) {
        self.set_color(Color::black());
    }

    fn needs_page_break(&self, extra: f32) -> bool {
        self.y - extra < self.layout.margin_bottom
    }

    fn end_text_block(&mut self) {
        self.current.extend_from_slice(b"ET\n");
    }

    fn add_page_number(&mut self) {
        let label = format!("Page {}", self.page_number);
        let x = self.layout.margin_left + self.layout.content_width() / 2.0 - 20.0;
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
        self.begin_page();
    }

    fn emit_line(&mut self, text: &str, font_size: f32) {
        self.emit_line_aligned(text, font_size, TextAlign::Left);
    }

    fn emit_line_aligned(&mut self, text: &str, font_size: f32, align: TextAlign) {
        let lh = line_height(font_size);
        if self.needs_page_break(lh) {
            self.new_page();
        }
        self.set_font(font_size);

        let x = match align {
            TextAlign::Left => self.layout.margin_left,
            TextAlign::Center => {
                let approx_width = self.estimate_text_width(text, font_size);
                self.layout.margin_left + (self.layout.content_width() - approx_width) / 2.0
            }
            TextAlign::Right => {
                let approx_width = self.estimate_text_width(text, font_size);
                self.layout.margin_left + self.layout.content_width() - approx_width
            }
            TextAlign::Justify => self.layout.margin_left,
        };

        self.current
            .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, self.y).as_bytes());
        self.current
            .extend_from_slice(format!("{} Tj\n", self.encode_text_for_current_font(text)).as_bytes());
        self.y -= lh;
    }

    fn encode_text_for_current_font(&self, text: &str) -> String {
        if self.current_font != FONT_COURIER {
            if let Some(encoder) = &self.unicode_font_encoder {
                if !use_base14_normalization() {
                    return encoder.encode_text_as_glyph_ids(text);
                }
            }
        }
        encode_pdf_text(text)
    }

    fn emit_colored_line(&mut self, text: &str, font_size: f32, color: Color) {
        self.set_color(color);
        self.emit_line(text, font_size);
        self.reset_color();
    }

    fn emit_empty_line(&mut self) {
        let lh = line_height(self.base_font_size) * 0.5;
        if self.needs_page_break(lh) {
            self.new_page();
        }
        self.y -= lh;
    }

    fn emit_horizontal_rule(&mut self) {
        // Add spacing above the rule
        self.y -= line_height(self.base_font_size) / 2.0;

        // Check for page break
        if self.needs_page_break(line_height(self.base_font_size)) {
            self.new_page();
        }

        // Draw a horizontal line across the content area
        let x1 = self.layout.margin_left;
        let x2 = self.layout.margin_left + self.layout.content_width();
        let y = self.y;
        let line_width = 1.0;
        let color = Color::gray();

        self.draw_line(x1, y, x2, y, line_width, color);

        // Add spacing below the rule
        self.y -= line_height(self.base_font_size);
    }

    fn finish(mut self) -> Vec<Vec<u8>> {
        self.end_text_block();
        self.pages.push(self.current);
        self.pages
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
    let unicode_font_support = if document_requires_unicode(elements) {
        prepare_unicode_font_support()
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
    );
    render_elements_to_builder(&mut builder, elements, base_font_size);
    let page_streams = builder.finish();
    assemble_pdf(
        filename,
        &page_streams,
        font,
        &layout,
        unicode_font_support.as_ref().map(|(bytes, _)| bytes.as_slice()),
        None,
    )?;
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
    let unicode_font_support = if document_requires_unicode(elements) {
        prepare_unicode_font_support()
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
    );
    render_elements_to_builder(&mut builder, elements, base_font_size);
    let page_streams = builder.finish();
    assemble_pdf(
        filename,
        &page_streams,
        font,
        &layout,
        unicode_font_support.as_ref().map(|(bytes, _)| bytes.as_slice()),
        compression_level,
    )?;
    Ok(())
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
                let align = if *level == 1 { TextAlign::Center } else { TextAlign::Left };
                builder.emit_empty_line();
                builder.set_font_with_style(fs, true, false);
                builder.emit_line_aligned(text, fs, align);
                builder.set_font_with_style(base_font_size, false, false);
                builder.emit_empty_line();
            }
            Element::Paragraph { text } => {
                builder.emit_wrapped_text(text, base_font_size);
            }
            Element::RichParagraph { segments } => {
                // Keep one callflow for better line wrapping across mixed inline segments
                // (plain/bold/italic/code/link/inline-math in the same paragraph).
                let mut combined = String::new();
                for segment in segments {
                    match segment {
                        TextSegment::Plain(text)
                        | TextSegment::Bold(text)
                        | TextSegment::Italic(text)
                        | TextSegment::BoldItalic(text) => combined.push_str(text),
                        TextSegment::Code(code) => {
                            combined.push('`');
                            combined.push_str(code);
                            combined.push('`');
                        }
                        TextSegment::MathInline(expr) => {
                            combined.push_str(&render_math_text(expr));
                        }
                        TextSegment::Link { text, url } => {
                            combined.push_str(text);
                            combined.push_str(" (");
                            combined.push_str(url);
                            combined.push(')');
                        }
                    }
                }
                builder.set_font_with_style(base_font_size, false, false);
                builder.emit_wrapped_text(&combined, base_font_size);
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
                let all_lines: Vec<&str> = code.lines().collect();

                builder.emit_empty_line();

                // Split code block across pages if needed
                let mut line_idx = 0;
                while line_idx < all_lines.len() {
                    // Calculate how many lines fit on current page
                    let available = builder.y - builder.layout.margin_bottom - padding * 2.0;
                    let max_lines_on_page = (available / line_h).floor() as usize;
                    let max_lines_on_page = max_lines_on_page.max(1);
                    let chunk_end = (line_idx + max_lines_on_page).min(all_lines.len());
                    let chunk = &all_lines[line_idx..chunk_end];
                    let chunk_height = chunk.len() as f32 * line_h + padding * 2.0;

                    // Account for top padding before drawing — shift y down by padding
                    builder.y -= padding;

                    // Draw background rectangle (from current y down by text height + bottom padding)
                    let text_block_height = chunk.len() as f32 * line_h;
                    let bg_color = Color::rgb(0.95, 0.95, 0.95);
                    let rect_x = builder.layout.margin_left - padding;
                    let rect_y = builder.y - text_block_height - padding;
                    let rect_width = builder.layout.content_width() + padding * 2.0;
                    let rect_height = chunk_height;
                    builder.draw_rectangle(rect_x, rect_y, rect_width, rect_height, bg_color);

                    // Draw border
                    let border_color = Color::rgb(0.75, 0.75, 0.75);
                    builder.draw_line(rect_x, rect_y, rect_x + rect_width, rect_y, 0.5, border_color);
                    builder.draw_line(rect_x, rect_y + rect_height, rect_x + rect_width, rect_y + rect_height, 0.5, border_color);
                    builder.draw_line(rect_x, rect_y, rect_x, rect_y + rect_height, 0.5, border_color);
                    builder.draw_line(rect_x + rect_width, rect_y, rect_x + rect_width, rect_y + rect_height, 0.5, border_color);

                    // Set monospace font
                    builder.set_monospace_font(code_size);

                    // Emit code lines with per-line syntax highlighting
                    for code_line in chunk {
                        let line_tokens = highlight_code(code_line, language);

                        if line_tokens.is_empty() || line_tokens.iter().all(|t| t.text.is_empty()) {
                            // Empty line or no tokens — just advance
                            builder.current.extend_from_slice(
                                format!("{} {} {} rg\n", 0.15, 0.15, 0.15).as_bytes()
                            );
                            builder.current.extend_from_slice(
                                format!("1 0 0 1 {} {} Tm\n", builder.layout.margin_left, builder.y).as_bytes()
                            );
                            builder.current.extend_from_slice(
                                format!("{} Tj\n", builder.encode_text_for_current_font(code_line)).as_bytes()
                            );
                        } else {
                            // Render each token with its color
                            let mut x_offset = builder.layout.margin_left;
                            for token in &line_tokens {
                                if token.text.is_empty() { continue; }
                                builder.current.extend_from_slice(
                                    format!("{} {} {} rg\n", token.color.r, token.color.g, token.color.b).as_bytes()
                                );
                                builder.current.extend_from_slice(
                                    format!("1 0 0 1 {} {} Tm\n", x_offset, builder.y).as_bytes()
                                );
                                builder.current.extend_from_slice(
                                    format!("{} Tj\n", builder.encode_text_for_current_font(&token.text)).as_bytes()
                                );
                                x_offset += estimated_text_width(&token.text, code_size, true);
                            }
                        }
                        builder.y -= line_h;
                    }

                    // Account for bottom padding
                    builder.y -= padding;

                    line_idx = chunk_end;

                    // If more lines remain, start a new page
                    if line_idx < all_lines.len() {
                        builder.set_font_with_style(base_font_size, false, false);
                        builder.reset_color();
                        builder.new_page();
                    }
                }

                // Reset to normal font and color
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
                builder.emit_wrapped_text(&format!("[Image: {}] ({})", alt, path), base_font_size);
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
                // Slightly larger display-math sizing improves readability of
                // large operators (∫, ∑, ∏) and script limits.
                let math_size = base_font_size * 1.22;
                let padding = 10.0;
                let line_h = line_height(math_size);
                let math_lines: Vec<&str> = expression.lines().collect();
                let block_height = math_lines.len() as f32 * line_h + padding * 2.0;

                builder.emit_empty_line();

                // Check page break
                if builder.needs_page_break(block_height) {
                    builder.new_page();
                }

                // Draw light blue background
                let bg_color = Color::rgb(0.93, 0.95, 1.0);
                let rect_x = builder.layout.margin_left - padding;
                let rect_y = builder.y - block_height;
                let rect_width = builder.layout.content_width() + padding * 2.0;
                builder.draw_rectangle(rect_x, rect_y, rect_width, block_height, bg_color);

                // Draw left accent border
                let accent_color = Color::rgb(0.3, 0.4, 0.8);
                builder.draw_line(rect_x, rect_y, rect_x, rect_y + block_height, 2.0, accent_color);

                // Render math expression in italic
                builder.set_font_with_style(math_size, false, true);
                builder.set_color(Color::rgb(0.1, 0.1, 0.3));
                for math_line in &math_lines {
                    // Render math symbols with text representation
                    let rendered = render_math_text(math_line);
                    builder.current.extend_from_slice(
                        format!("1 0 0 1 {} {} Tm\n", builder.layout.margin_left + 4.0, builder.y).as_bytes()
                    );
                    builder.current.extend_from_slice(
                        format!("{} Tj\n", builder.encode_text_for_current_font(&rendered)).as_bytes()
                    );
                    builder.y -= line_h;
                }

                builder.set_font_with_style(base_font_size, false, false);
                builder.reset_color();
                builder.emit_empty_line();
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

fn add_shared_font_resources(generator: &mut PdfGenerator, unicode_font_bytes: Option<&[u8]>) -> FontResourceIds {
    let helvetica_id = if let Some(bytes) = unicode_font_bytes {
        let font_file_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", bytes.len()),
            bytes.to_vec(),
        );

        let descriptor_id = generator.add_object(format!(
            "<< /Type /FontDescriptor\n/FontName /UnicodeTT\n/Flags 4\n/FontBBox [0 -200 1000 900]\n/ItalicAngle 0\n/Ascent 800\n/Descent -200\n/CapHeight 700\n/StemV 80\n/MissingWidth 1000\n/FontFile2 {} 0 R\n>>\n",
            font_file_id
        ));

        let cid_font_id = generator.add_object(format!(
            "<< /Type /Font\n/Subtype /CIDFontType2\n/BaseFont /UnicodeTT\n/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >>\n/FontDescriptor {} 0 R\n/DW 700\n/CIDToGIDMap /Identity\n>>\n",
            descriptor_id
        ));

        generator.add_object(format!(
            "<< /Type /Font\n/Subtype /Type0\n/BaseFont /UnicodeTT\n/Encoding /Identity-H\n/DescendantFonts [{} 0 R]\n>>\n",
            cid_font_id
        ))
    } else {
        generator.add_object(format!(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
            FONT_HELVETICA
        ))
    };

    let (helvetica_bold_id, helvetica_oblique_id, helvetica_bold_oblique_id) =
        if unicode_font_bytes.is_some() {
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

/// Generate PDF bytes from elements (library API — no filesystem access needed)
pub fn generate_pdf_bytes(
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
) -> Result<Vec<u8>> {
    let unicode_font_support = if document_requires_unicode(elements) {
        prepare_unicode_font_support()
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
    );
    render_elements_to_builder(&mut builder, elements, base_font_size);
    let page_streams = builder.finish();
    Ok(assemble_pdf_bytes(
        &page_streams,
        font,
        &layout,
        unicode_font_support.as_ref().map(|(bytes, _)| bytes.as_slice()),
        None,
        None,
    ))
}

/// Same as `generate_pdf_bytes` but with optional stream compression
pub fn generate_pdf_bytes_with_compression(
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
    compression_level: Option<u8>,
) -> Result<Vec<u8>> {
    let unicode_font_support = if document_requires_unicode(elements) {
        prepare_unicode_font_support()
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
    );
    render_elements_to_builder(&mut builder, elements, base_font_size);
    let page_streams = builder.finish();
    Ok(assemble_pdf_bytes(
        &page_streams,
        font,
        &layout,
        unicode_font_support.as_ref().map(|(bytes, _)| bytes.as_slice()),
        compression_level,
        None,
    ))
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
    let unicode_font_support = if document_requires_unicode(elements) {
        prepare_unicode_font_support()
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
    );
    render_elements_to_builder(&mut builder, elements, base_font_size);
    let page_streams = builder.finish();
    Ok(assemble_pdf_bytes(
        &page_streams,
        font,
        &layout,
        unicode_font_support.as_ref().map(|(bytes, _)| bytes.as_slice()),
        None,
        Some(&options),
    ))
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
    let unicode_font_support = if document_requires_unicode(elements) {
        prepare_unicode_font_support()
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
    );
    render_elements_to_builder(&mut builder, elements, base_font_size);
    let all_page_streams = builder.finish();

    if range.start >= all_page_streams.len() {
        anyhow::bail!(
            "Start page {} exceeds total pages {}",
            range.start,
            all_page_streams.len()
        );
    }
    let end = range.end.min(all_page_streams.len());
    let selected = &all_page_streams[range.start..end];

    Ok(assemble_pdf_bytes(
        selected,
        font,
        &layout,
        unicode_font_support.as_ref().map(|(bytes, _)| bytes.as_slice()),
        None,
        None,
    ))
}

/// Assemble final PDF bytes from per-page content streams
fn assemble_pdf_bytes(
    page_streams: &[Vec<u8>],
    _font: &str,
    layout: &PageLayout,
    unicode_font_bytes: Option<&[u8]>,
    compression_level: Option<u8>,
    accessibility: Option<&AccessibilityOptions>,
) -> Vec<u8> {
    let mut generator = PdfGenerator::new();

    let font_ids = add_shared_font_resources(&mut generator, unicode_font_bytes);

    let mut page_ids = Vec::new();

    // Objects now are: shared font objects first, then each page contributes
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
             >> >>\n\
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

    // Build catalog with optional tagged PDF entries
    let mut catalog_entries = format!("/Pages {} 0 R\n", actual_pages_id);
    if let Some(opts) = accessibility {
        if opts.tagged_pdf {
            catalog_entries.push_str("/MarkInfo << /Marked true >>\n");
            catalog_entries.push_str(&format!("/Lang ({})\n", escape_pdf_meta(&opts.language)));
            if let Some(st_id) = struct_tree_id {
                catalog_entries.push_str(&format!("/StructTreeRoot {} 0 R\n", st_id));
            }
        }
    }

    let catalog_dict = format!(
        "<< /Type /Catalog\n\
         {}\
         >>\n",
        catalog_entries
    );
    generator.add_object(catalog_dict);

    generator.generate()
}

/// Assemble final PDF from per-page content streams and write to file
fn assemble_pdf(
    filename: &str,
    page_streams: &[Vec<u8>],
    font: &str,
    layout: &PageLayout,
    unicode_font_bytes: Option<&[u8]>,
    compression_level: Option<u8>,
) -> Result<()> {
    let pdf_data = assemble_pdf_bytes(page_streams, font, layout, unicode_font_bytes, compression_level, None);
    let mut file = File::create(filename)?;
    file.write_all(&pdf_data)?;
    Ok(())
}


// --- Accessibility / Tagged PDF support ---

/// Accessibility options for PDF generation
#[derive(Debug, Clone)]
pub struct AccessibilityOptions {
    /// Enable tagged PDF (PDF/UA compliance)
    pub tagged_pdf: bool,
    /// Document language (e.g., "en-US", "en-GB")
    pub language: String,
    /// Document title for accessibility
    pub title: Option<String>,
}

impl Default for AccessibilityOptions {
    fn default() -> Self {
        Self {
            tagged_pdf: false,
            language: "en".to_string(),
            title: None,
        }
    }
}

impl AccessibilityOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tagged_pdf(mut self, tagged: bool) -> Self {
        self.tagged_pdf = tagged;
        self
    }

    pub fn with_language(mut self, lang: String) -> Self {
        self.language = lang;
        self
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }
}

/// Structure element types for tagged PDF
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureType {
    Document,
    Part,
    Art,
    Sect,
    Div,
    BlockQuote,
    Caption,
    TOC,
    TOCI,
    Index,
    NonStruct,
    Private,
    P,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    L,
    LI,
    Lbl,
    LBody,
    Table,
    TR,
    TH,
    TD,
    THead,
    TBody,
    TFoot,
    Span,
    Quote,
    Note,
    Reference,
    BibEntry,
    Code,
    Link,
    Figure,
    Formula,
}

impl StructureType {
    /// Get the PDF structure type name as per PDF 1.7 specification
    pub fn as_pdf_name(&self) -> &str {
        match self {
            Self::Document => "Document",
            Self::Part => "Part",
            Self::Art => "Art",
            Self::Sect => "Sect",
            Self::Div => "Div",
            Self::BlockQuote => "BlockQuote",
            Self::Caption => "Caption",
            Self::TOC => "TOC",
            Self::TOCI => "TOCI",
            Self::Index => "Index",
            Self::NonStruct => "NonStruct",
            Self::Private => "Private",
            Self::P => "P",
            Self::H1 => "H1",
            Self::H2 => "H2",
            Self::H3 => "H3",
            Self::H4 => "H4",
            Self::H5 => "H5",
            Self::H6 => "H6",
            Self::L => "L",
            Self::LI => "LI",
            Self::Lbl => "Lbl",
            Self::LBody => "LBody",
            Self::Table => "Table",
            Self::TR => "TR",
            Self::TH => "TH",
            Self::TD => "TD",
            Self::THead => "THead",
            Self::TBody => "TBody",
            Self::TFoot => "TFoot",
            Self::Span => "Span",
            Self::Quote => "Quote",
            Self::Note => "Note",
            Self::Reference => "Reference",
            Self::BibEntry => "BibEntry",
            Self::Code => "Code",
            Self::Link => "Link",
            Self::Figure => "Figure",
            Self::Formula => "Formula",
        }
    }
}

/// Structure element for tagged PDF
#[derive(Debug, Clone)]
pub struct StructureElement {
    pub struct_type: StructureType,
    pub alt_text: Option<String>,
    pub actual_text: Option<String>,
    pub children: Vec<StructureElement>,
    pub content_id: Option<u32>, // Reference to content object
}

impl StructureElement {
    pub fn new(struct_type: StructureType) -> Self {
        Self {
            struct_type,
            alt_text: None,
            actual_text: None,
            children: Vec::new(),
            content_id: None,
        }
    }

    pub fn with_alt_text(mut self, text: String) -> Self {
        self.alt_text = Some(text);
        self
    }

    pub fn with_actual_text(mut self, text: String) -> Self {
        self.actual_text = Some(text);
        self
    }

    pub fn with_children(mut self, children: Vec<StructureElement>) -> Self {
        self.children = children;
        self
    }

    pub fn add_child(&mut self, child: StructureElement) {
        self.children.push(child);
    }

    pub fn with_content_id(mut self, id: u32) -> Self {
        self.content_id = Some(id);
        self
    }

    /// Generate the structure element dictionary for PDF
    pub fn to_pdf_dict(&self, obj_id: u32) -> String {
        let mut dict = format!("<< /Type /StructElem /S /{}", self.struct_type.as_pdf_name());

        if let Some(ref alt) = self.alt_text {
            dict.push_str(&format!(" /Alt {}", escape_pdf_string(alt)));
        }

        if let Some(ref actual) = self.actual_text {
            dict.push_str(&format!(" /A {}", escape_pdf_string(actual)));
        }

        if let Some(ref content_id) = self.content_id {
            dict.push_str(&format!(" /K {} 0 R", content_id));
        } else if !self.children.is_empty() {
            let kid_refs: Vec<String> = self.children.iter()
                .enumerate()
                .map(|(i, _)| format!("{} 0 R", obj_id + 1 + i as u32))
                .collect();
            dict.push_str(&format!(" /K [{}]", kid_refs.join(" ")));
        } else {
            dict.push_str(" /K 0"); // No content
        }

        dict.push_str(" >>");
        dict
    }
}

/// Convert Element to StructureElement for accessibility
pub fn element_to_structure(element: &Element) -> StructureElement {
    match element {
        Element::Heading { level, text } => {
            let struct_type = match level {
                1 => StructureType::H1,
                2 => StructureType::H2,
                3 => StructureType::H3,
                4 => StructureType::H4,
                5 => StructureType::H5,
                _ => StructureType::H6,
            };
            StructureElement::new(struct_type)
                .with_actual_text(text.clone())
        }
        Element::Paragraph { text } => {
            StructureElement::new(StructureType::P)
                .with_actual_text(text.clone())
        }
        Element::RichParagraph { segments } => {
            let text = segments.iter().map(|s| match s {
                TextSegment::Plain(t) | TextSegment::Bold(t) | TextSegment::Italic(t) | TextSegment::BoldItalic(t) => t.clone(),
                TextSegment::Code(c) => format!("`{}`", c),
                TextSegment::MathInline(expr) => render_math_text(expr),
                TextSegment::Link { text, url } => format!("{} ({})", text, url),
            }).collect::<Vec<_>>().join("");
            StructureElement::new(StructureType::P)
                .with_actual_text(text)
        }
        Element::UnorderedListItem { text, .. } | Element::OrderedListItem { text, .. } | Element::TaskListItem { text, .. } => {
            StructureElement::new(StructureType::LI)
                .with_actual_text(text.clone())
        }
        Element::CodeBlock { code, .. } => {
            StructureElement::new(StructureType::Code)
                .with_actual_text(code.clone())
        }
        Element::BlockQuote { text, .. } => {
            StructureElement::new(StructureType::BlockQuote)
                .with_actual_text(text.clone())
        }
        Element::TableRow { .. } => {
            StructureElement::new(StructureType::TR)
        }
        Element::HorizontalRule => {
            StructureElement::new(StructureType::NonStruct)
        }
        Element::EmptyLine => {
            StructureElement::new(StructureType::NonStruct)
        }
        Element::Footnote { .. } => {
            StructureElement::new(StructureType::Note)
        }
        Element::DefinitionItem { .. } => {
            StructureElement::new(StructureType::Div)
        }
        Element::InlineCode { code } => {
            StructureElement::new(StructureType::Code)
                .with_actual_text(code.clone())
        }
        Element::Link { text, url } => {
            StructureElement::new(StructureType::Link)
                .with_actual_text(format!("{} ({})", text, url))
        }
        Element::Image { alt, .. } => {
            StructureElement::new(StructureType::Figure)
                .with_alt_text(alt.clone())
        }
        Element::StyledText { text, .. } => {
            StructureElement::new(StructureType::Span)
                .with_actual_text(text.clone())
        }
        Element::MathBlock { expression } => {
            StructureElement::new(StructureType::Formula)
                .with_actual_text(expression.clone())
        }
        Element::MathInline { expression } => {
            StructureElement::new(StructureType::Formula)
                .with_actual_text(expression.clone())
        }
        Element::PageBreak => {
            StructureElement::new(StructureType::NonStruct)
        }
    }
}

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
        assert!(rendered.contains("(sin x)/(x)"), "rendered: {}", rendered);
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
        assert!(expr1.contains("x²"), "Should render x squared: {}", expr1);
        
        let expr2 = render_math_text(r"\prod_{k=1}^{m} b_k");
        assert!(expr2.contains("∏"), "Should contain product symbol: {}", expr2);
        assert!(expr2.contains("bₖ"), "Should render b subscript k: {}", expr2);
        
        let expr3 = render_math_text(r"\forall x \in \mathbb{R}, x \geq 0 \Rightarrow \sqrt{x} \in \mathbb{R}");
        assert!(expr3.contains("∀"), "Should contain forall: {}", expr3);
        assert!(expr3.contains("∈"), "Should contain element of: {}", expr3);
        assert!(expr3.contains("ℝ"), "Should contain real numbers: {}", expr3);
        assert!(expr3.contains("≥"), "Should contain greater or equal: {}", expr3);
        assert!(expr3.contains("⇒"), "Should contain implies: {}", expr3);
        assert!(expr3.contains("√"), "Should contain square root: {}", expr3);
    }

    #[test]
    fn test_estimated_text_width_unicode_is_wider_than_ascii() {
        let ascii = estimated_text_width("Hello", 12.0, false);
        let cjk = estimated_text_width("你好你好", 12.0, false);
        assert!(cjk > ascii);
    }

    #[test]
    fn test_embeds_unicode_type0_font_when_available() {
        if prepare_unicode_font_support().is_none() {
            // Environment without a Unicode TTF candidate; skip.
            return;
        }

        let elements = vec![
            Element::Paragraph { text: "Unicode: 你好 Γεια 😀 ∑".into() },
        ];
        let bytes = generate_pdf_bytes(&elements, "Helvetica", 12.0, PageLayout::portrait()).unwrap();
        let raw = String::from_utf8_lossy(&bytes);

        assert!(raw.contains("/Subtype /Type0"), "Expected Type0 font for unicode text");
        assert!(raw.contains("/Subtype /CIDFontType2"), "Expected CIDFontType2 descendant font");
        assert!(raw.contains("/FontFile2"), "Expected embedded FontFile2 object");
        assert!(raw.contains("/DW 700"), "Expected balanced CID default width to avoid overlap while keeping spacing tight");
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
}
