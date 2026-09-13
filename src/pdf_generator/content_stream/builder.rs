//! Core `ContentStreamBuilder` state and low-level operator emission.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::elements::PageNumberStyle;
use crate::image::ImageInfo;
use crate::thesis::{CitationRegistry, format_folio};

use super::{
    ContentStreamBuilder, FONT_COURIER, FONT_HELVETICA, FONT_HELVETICA_BOLD,
    FONT_HELVETICA_BOLD_OBLIQUE, FONT_HELVETICA_OBLIQUE, OutlineDest,
};
use crate::pdf_generator::layout::{
    Color, PageLayout, TextAlign, estimated_text_width, line_height,
};
use crate::pdf_generator::text_support::{encode_pdf_text, use_base14_normalization};
use crate::pdf_generator::unicode_support::UnicodeFontEncoder;

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

    pub(super) fn content_left(&self) -> f32 {
        self.layout.column_left(self.current_column)
    }

    pub(super) fn content_width(&self) -> f32 {
        self.layout.column_width()
    }

    pub(super) fn mark_content_placed(&mut self) {
        self.content_placed_on_page = true;
    }

    pub(super) fn set_font(&mut self, size: f32) {
        self.set_font_with_style(size, self.current_font_bold, self.current_font_italic);
    }

    pub(super) fn set_font_with_style(&mut self, size: f32, bold: bool, italic: bool) {
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

    pub(super) fn set_monospace_font(&mut self, size: f32) {
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
        if self.unicode_font_encoder.is_some()
            && !use_base14_normalization()
            && let Some(enc) = &self.unicode_font_encoder
        {
            return enc.estimate_width(text, font_size);
        }
        estimated_text_width(text, font_size, true)
    }

    /// Wrap a code line to the content width (space-aware, then hard-wrap).
    pub(super) fn wrap_code_line(&self, line: &str, max_width: f32, font_size: f32) -> Vec<String> {
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

    pub(super) fn draw_rectangle(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        fill_color: Color,
    ) {
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

    pub(super) fn draw_line(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        line_width: f32,
        color: Color,
    ) {
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

    /// Approximate text width for wrapping calculations
    pub(super) fn estimate_text_width(&self, text: &str, font_size: f32) -> f32 {
        if self.current_font != FONT_COURIER
            && let Some(encoder) = &self.unicode_font_encoder
            && !use_base14_normalization()
        {
            return encoder.estimate_width(text, font_size);
        }
        estimated_text_width(text, font_size, self.current_font == FONT_COURIER)
    }

    pub(super) fn set_color(&mut self, color: Color) {
        self.current_color = color;
        self.current
            .extend_from_slice(format!("{} {} {} rg\n", color.r, color.g, color.b).as_bytes());
    }

    pub(super) fn reset_color(&mut self) {
        self.set_color(Color::black());
    }

    pub(super) fn end_text_block(&mut self) {
        self.current.extend_from_slice(b"ET\n");
    }

    pub(super) fn emit_line(&mut self, text: &str, font_size: f32) {
        self.emit_line_aligned(text, font_size, TextAlign::Left);
    }

    pub(super) fn emit_line_aligned(&mut self, text: &str, font_size: f32, align: TextAlign) {
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
    pub(super) fn emit_full_width_heading(&mut self, text: &str, font_size: f32) {
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

    pub(super) fn encode_text_for_current_font(&self, text: &str) -> String {
        if self.current_font != FONT_COURIER
            && let Some(encoder) = &self.unicode_font_encoder
            && !use_base14_normalization()
        {
            return encoder.encode_text_as_glyph_ids(text);
        }
        encode_pdf_text(text)
    }

    pub(super) fn emit_empty_line(&mut self) {
        let lh = line_height(self.base_font_size) * 0.5;
        self.ensure_space(lh);
        self.y -= lh;
    }

    pub(super) fn emit_horizontal_rule(&mut self) {
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

    #[allow(clippy::type_complexity)]
    pub(crate) fn finish(mut self) -> (Vec<Vec<u8>>, Vec<OutlineDest>, Vec<(String, ImageInfo)>) {
        self.end_text_block();
        if self.show_page_numbers {
            self.add_page_number();
        }
        self.pages.push(self.current);
        (self.pages, self.outlines, self.images)
    }

    pub(super) fn push_outline(&mut self, title: &str, level: u8) {
        let page_label = format_folio(self.page_num_style, self.folio).unwrap_or_default();
        self.outlines.push(OutlineDest {
            title: title.to_string(),
            level,
            page_index: (self.page_number as usize).saturating_sub(1),
            y: self.y,
            page_label,
        });
    }

    pub(super) fn set_page_number_style(&mut self, style: PageNumberStyle) {
        if self.page_num_style == style {
            return;
        }
        self.page_num_style = style;
        if style != PageNumberStyle::None {
            self.folio = 1;
        }
    }

    pub(super) fn citation_marker(&mut self, key: &str) -> String {
        let n = self.citations.number_for(key);
        format!("[{}]", n)
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::unicode_support::prepare_unicode_font_support;
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
