//! Content stream builder: cursor management, page breaks, font switches,
//! and element-to-stream rendering.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::elements::{ChartKind, Element, PageNumberStyle, TextSegment};
use crate::image::{self, ImageInfo};
use crate::table_renderer::{PdfTableHelper, TableStyle};
use crate::thesis::{
    CitationRegistry, build_bibliography_elements, collect_citation_defs, expand_toc, format_folio,
};

use super::code_highlight::highlight_code;
use super::generate_pdf_bytes_internal_with_base;
use super::layout::{
    Color, PageLayout, TextAlign, estimated_text_width, heading_font_size, line_height,
    split_long_word_for_wrap,
};
use super::math_layout::{MathPiece, line_height_for_pieces, parse_display_math, piece_width};
use super::text_support::{encode_pdf_text, render_math_text, use_base14_normalization};
use super::unicode_support::UnicodeFontEncoder;
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

impl ContentStreamBuilder {
    pub(crate) fn new(
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
        self.current
            .extend_from_slice(b"0.75 0.75 0.75 RG\n0.4 w\n");
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
            self.current
                .extend_from_slice(format!("{} {} {} RG\n", color.r, color.g, color.b).as_bytes());
            self.current.extend_from_slice(b"0.4 w\n");
            self.current
                .extend_from_slice(format!("{} {} m {} {} l S\n", x, bottom, x, top).as_bytes());
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
            format!("{} {} {} rg\n", fill_color.r, fill_color.g, fill_color.b).as_bytes(),
        );

        // Draw and fill rectangle
        self.current
            .extend_from_slice(format!("{} {} {} {} re f\n", x, y, width, height).as_bytes());

        // Resume text block
        self.current.extend_from_slice(b"BT\n");
        self.set_font(self.current_font_size);
        // Always reset to black text after drawing rectangle
        self.current_color = Color::black();
        self.current
            .extend_from_slice("0 0 0 rg\n".to_string().as_bytes());
    }

    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, line_width: f32, color: Color) {
        // End text block temporarily to draw line
        self.current.extend_from_slice(b"ET\n");

        // Set stroke color and line width
        self.current
            .extend_from_slice(format!("{} {} {} RG\n", color.r, color.g, color.b).as_bytes());
        self.current
            .extend_from_slice(format!("{} w\n", line_width).as_bytes());

        // Draw line
        self.current
            .extend_from_slice(format!("{} {} m {} {} l S\n", x1, y1, x2, y2).as_bytes());

        // Resume text block
        self.current.extend_from_slice(b"BT\n");
        self.set_font(self.current_font_size);
        // Reset to current text color
        self.current.extend_from_slice(
            format!(
                "{} {} {} rg\n",
                self.current_color.r, self.current_color.g, self.current_color.b
            )
            .as_bytes(),
        );
    }

    /// Render a complete table with borders, text wrapping, and alignment
    fn render_table(
        &mut self,
        rows: &[Vec<String>],
        base_font_size: f32,
        alignments: Option<&[crate::elements::TableAlignment]>,
    ) {
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
        let approx_char_width =
            if self.unicode_font_encoder.is_some() && !use_base14_normalization() {
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

        // Draw cell background fills (header + zebra striping) before borders
        self.current.extend_from_slice(b"ET\n");
        let mut row_y = start_y;
        for (row_idx, &row_h) in dims.row_heights.iter().enumerate() {
            let bg = if row_idx == 0 {
                style.header_bg_color
            } else if style.zebra_striping && row_idx % 2 == 0 {
                Some(style.alt_row_bg_color)
            } else {
                None
            };
            if let Some((r, g, b)) = bg {
                self.current
                    .extend_from_slice(format!("{} {} {} rg\n", r, g, b).as_bytes());
                self.current.extend_from_slice(
                    format!(
                        "{} {} {} {} re f\n",
                        start_x,
                        row_y - row_h,
                        dims.total_width,
                        row_h
                    )
                    .as_bytes(),
                );
            }
            row_y -= row_h;
        }

        // Draw outer border
        let (br, bg, bb) = style.border_color;
        self.current
            .extend_from_slice(format!("{} {} {} RG\n", br, bg, bb).as_bytes());
        self.current
            .extend_from_slice(format!("{} w\n", style.border_width).as_bytes());
        self.current.extend_from_slice(
            format!(
                "{} {} m {} {} l S\n",
                start_x,
                start_y,
                start_x + dims.total_width,
                start_y
            )
            .as_bytes(),
        );
        self.current.extend_from_slice(
            format!(
                "{} {} m {} {} l S\n",
                start_x,
                start_y - dims.total_height,
                start_x + dims.total_width,
                start_y - dims.total_height
            )
            .as_bytes(),
        );
        self.current.extend_from_slice(
            format!(
                "{} {} m {} {} l S\n",
                start_x,
                start_y,
                start_x,
                start_y - dims.total_height
            )
            .as_bytes(),
        );
        self.current.extend_from_slice(
            format!(
                "{} {} m {} {} l S\n",
                start_x + dims.total_width,
                start_y,
                start_x + dims.total_width,
                start_y - dims.total_height
            )
            .as_bytes(),
        );

        // Draw horizontal grid lines
        let mut current_y = start_y;
        for (i, &row_h) in dims.row_heights.iter().enumerate() {
            if i > 0 {
                let (gr, gg, gb) = style.grid_color;
                self.current
                    .extend_from_slice(format!("{} {} {} RG\n", gr, gg, gb).as_bytes());
                self.current
                    .extend_from_slice(format!("{} w\n", style.grid_line_width).as_bytes());
                self.current.extend_from_slice(
                    format!(
                        "{} {} m {} {} l S\n",
                        start_x,
                        current_y,
                        start_x + dims.total_width,
                        current_y
                    )
                    .as_bytes(),
                );
            }
            current_y -= row_h;
        }

        // Draw vertical grid lines
        let mut current_x = start_x;
        for i in 1..dims.num_cols {
            current_x += dims.column_widths[i - 1];
            let (gr, gg, gb) = style.grid_color;
            self.current
                .extend_from_slice(format!("{} {} {} RG\n", gr, gg, gb).as_bytes());
            self.current
                .extend_from_slice(format!("{} w\n", style.grid_line_width).as_bytes());
            self.current.extend_from_slice(
                format!(
                    "{} {} m {} {} l S\n",
                    current_x,
                    start_y,
                    current_x,
                    start_y - dims.total_height
                )
                .as_bytes(),
            );
        }

        // Resume text block
        self.current.extend_from_slice(b"BT\n");
        self.current.extend_from_slice(b"0 0 0 rg\n");

        // Draw cell contents with wrapping and alignment
        let mut row_y = start_y;
        for (row_idx, row) in table_rows.iter().enumerate() {
            // Use bold font for header row if configured
            if row_idx == 0 && style.header_text_bold {
                self.set_font_with_style(base_font_size, true, false);
            } else {
                self.set_font(base_font_size);
            }
            let mut col_x = start_x;
            for (col_idx, cell) in row.cells.iter().enumerate() {
                if col_idx >= dims.num_cols {
                    break;
                }
                let cell_width = dims.column_widths[col_idx];
                let cell_height = dims.row_heights[row_idx];
                let max_chars = ((cell_width - style.cell_padding * 2.0) / approx_char_width)
                    .floor()
                    .max(1.0) as usize;

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

                    self.current
                        .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, y).as_bytes());
                    self.current.extend_from_slice(
                        format!("{} Tj\n", self.encode_text_for_current_font(line)).as_bytes(),
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

        let approx_char_width =
            if self.unicode_font_encoder.is_some() && !use_base14_normalization() {
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
                    builder.draw_line(x, strike_y, x + w, strike_y, 0.7, Color::black());
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
        let flattened = super::text_support::flatten_math_environments(expression);
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
        self.draw_line(
            rect_x,
            rect_y,
            rect_x,
            rect_y + block_height,
            2.0,
            accent_color,
        );

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
            let mut x = self.content_left() + ((self.content_width() - total_w) / 2.0).max(4.0);

            for piece in pieces {
                match piece {
                    MathPiece::Text(text) => {
                        self.set_font_with_style(math_size, false, true);
                        self.current
                            .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, axis_y).as_bytes());
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
                                format!("1 0 0 1 {} {} Tm\n", x, axis_y - op_size * 0.18)
                                    .as_bytes(),
                            );
                            let enc = self.encode_text_for_current_font(&sym);
                            self.current
                                .extend_from_slice(format!("{} Tj\n", enc).as_bytes());

                            let sx = x + sym_w * 0.72;
                            if !upper.is_empty() {
                                self.set_font_with_style(script, false, false);
                                self.current.extend_from_slice(
                                    format!("1 0 0 1 {} {} Tm\n", sx, axis_y + script * 1.05)
                                        .as_bytes(),
                                );
                                let enc = self.encode_text_for_current_font(upper);
                                self.current
                                    .extend_from_slice(format!("{} Tj\n", enc).as_bytes());
                            }
                            if !lower.is_empty() {
                                self.set_font_with_style(script, false, false);
                                self.current.extend_from_slice(
                                    format!("1 0 0 1 {} {} Tm\n", sx, axis_y - script * 1.15)
                                        .as_bytes(),
                                );
                                let enc = self.encode_text_for_current_font(lower);
                                self.current
                                    .extend_from_slice(format!("{} Tj\n", enc).as_bytes());
                            }
                            let lim_w =
                                measure(self, lower, script).max(measure(self, upper, script));
                            x += sym_w + 4.0 + lim_w;
                        } else {
                            // Sum/prod: limits above and below, centered on symbol.
                            let lim_w =
                                measure(self, lower, script).max(measure(self, upper, script));
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
                            format!("1 0 0 1 {} {} Tm\n", cx - nw / 2.0, axis_y + script * 0.75)
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
                            format!("1 0 0 1 {} {} Tm\n", cx - dw / 2.0, axis_y - script * 0.95)
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
        self.current
            .extend_from_slice(format!("{} {} {} rg\n", color.r, color.g, color.b).as_bytes());
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
        self.current.extend_from_slice(
            format!("{} Tj\n", self.encode_text_for_current_font(&display)).as_bytes(),
        );
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
        let x = self.layout.margin_left + (self.layout.full_content_width() - approx_width) / 2.0;
        self.current
            .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, self.y).as_bytes());
        self.current.extend_from_slice(
            format!("{} Tj\n", self.encode_text_for_current_font(text)).as_bytes(),
        );
        self.y -= lh;
        self.column_top_y = self.y;
        self.mark_content_placed();
    }

    fn encode_text_for_current_font(&self, text: &str) -> String {
        if self.current_font != FONT_COURIER
            && let Some(encoder) = &self.unicode_font_encoder
            && !use_base14_normalization()
        {
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

    pub(crate) fn finish(mut self) -> (Vec<Vec<u8>>, Vec<OutlineDest>, Vec<(String, ImageInfo)>) {
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
                alt,
                resolved.display()
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
    fn emit_chart(&mut self, kind: ChartKind, title: &Option<String>, points: &[(String, f32)]) {
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
                format!(
                    "{} {} {} rg\n{:.2} {:.2} {:.2} {:.2} re f\n",
                    r, g, b, x, y, bar_w, h
                )
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
        let min_v = points.iter().map(|(_, v)| *v).fold(f32::INFINITY, f32::min);
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

    fn append_pie_slice(&mut self, cx: f32, cy: f32, radius: f32, a0: f32, a1: f32, fill: Color) {
        // Approximate arc with line segments.
        let steps = ((a1 - a0).abs() / 0.2).ceil().max(2.0) as usize;
        self.current.extend_from_slice(
            format!(
                "{} {} {} rg\n{:.2} {:.2} m\n",
                fill.r, fill.g, fill.b, cx, cy
            )
            .as_bytes(),
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
    label
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>()
        + "…"
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

/// Expand TOC (two-pass outlines) and attach citation definitions for rendering.
pub(crate) fn prepare_elements_for_render(
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
pub(crate) fn render_elements_to_builder(
    builder: &mut ContentStreamBuilder,
    elements: &[Element],
    base_font_size: f32,
) {
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut table_alignments: Option<Vec<crate::elements::TableAlignment>> = None;

    for elem in elements {
        // Handle table rows specially - accumulate them
        if let Element::TableRow {
            cells,
            is_separator,
            alignments,
        } = elem
        {
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
            Element::OrderedListItem {
                number,
                text,
                depth,
            } => {
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
                    builder.draw_line(
                        rect_x,
                        rect_y,
                        rect_x + rect_width,
                        rect_y,
                        0.5,
                        border_color,
                    );
                    builder.draw_line(
                        rect_x,
                        rect_y + rect_height,
                        rect_x + rect_width,
                        rect_y + rect_height,
                        0.5,
                        border_color,
                    );
                    builder.draw_line(
                        rect_x,
                        rect_y,
                        rect_x,
                        rect_y + rect_height,
                        0.5,
                        border_color,
                    );
                    builder.draw_line(
                        rect_x + rect_width,
                        rect_y,
                        rect_x + rect_width,
                        rect_y + rect_height,
                        0.5,
                        border_color,
                    );

                    builder.set_monospace_font(code_size);

                    for code_line in chunk {
                        let line_tokens = highlight_code(code_line, language);

                        if line_tokens.is_empty() || line_tokens.iter().all(|t| t.text.is_empty()) {
                            builder.current.extend_from_slice(
                                format!("{} {} {} rg\n", 0.15, 0.15, 0.15).as_bytes(),
                            );
                            builder.current.extend_from_slice(
                                format!("1 0 0 1 {} {} Tm\n", builder.content_left(), builder.y)
                                    .as_bytes(),
                            );
                            builder.current.extend_from_slice(
                                format!("{} Tj\n", builder.encode_text_for_current_font(code_line))
                                    .as_bytes(),
                            );
                        } else {
                            // Position once, then emit sequential Tj so extractors keep identifiers contiguous.
                            builder.current.extend_from_slice(
                                format!("1 0 0 1 {} {} Tm\n", builder.content_left(), builder.y)
                                    .as_bytes(),
                            );
                            for token in &line_tokens {
                                if token.text.is_empty() {
                                    continue;
                                }
                                builder.current.extend_from_slice(
                                    format!(
                                        "{} {} {} rg\n",
                                        token.color.r, token.color.g, token.color.b
                                    )
                                    .as_bytes(),
                                );
                                builder.current.extend_from_slice(
                                    format!(
                                        "{} Tj\n",
                                        builder.encode_text_for_current_font(&token.text)
                                    )
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
            Element::Chart {
                kind,
                title,
                points,
            } => {
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

#[cfg(test)]
mod tests {
    use super::super::unicode_support::prepare_unicode_font_support;
    use super::*;

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
