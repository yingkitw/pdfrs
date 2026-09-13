//! Page lifecycle: page starts/ends, running headers, column gutters,
//! column/page advancement, and folio numbering.

use crate::elements::PageNumberStyle;
use crate::thesis::format_folio;

use super::{ContentStreamBuilder, FONT_HELVETICA};
use crate::pdf_generator::layout::Color;
use crate::pdf_generator::text_support::{encode_pdf_text, use_base14_normalization};

impl ContentStreamBuilder {
    pub(super) fn begin_page(&mut self) {
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
            .extend_from_slice("0.35 0.35 0.35 rg\n".to_string().as_bytes());
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
    pub(super) fn ensure_space(&mut self, extra: f32) {
        if self.y - extra < self.layout.margin_bottom {
            self.advance_column_or_page();
        }
    }

    /// Switch column count mid-document. Starts a fresh page only if real
    /// content was already placed on the current page.
    pub(super) fn set_columns(&mut self, columns: u8) {
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

    pub(super) fn add_page_number(&mut self) {
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

    pub(super) fn new_page(&mut self) {
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
}
