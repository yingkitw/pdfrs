//! Display-math block rendering with stacked fractions and operator limits.

use super::ContentStreamBuilder;
use crate::pdf_generator::layout::{Color, estimated_text_width};
use crate::pdf_generator::math_layout::{
    MathPiece, line_height_for_pieces, parse_display_math, piece_width,
};
use crate::pdf_generator::text_support::use_base14_normalization;

impl ContentStreamBuilder {
    /// Render a display-math block with stacked fractions and operator limits.
    pub(super) fn emit_display_math(&mut self, expression: &str, base_font_size: f32) {
        let math_size = base_font_size * 1.28;
        let padding = 10.0;
        // Flatten multi-line matrix environments first, then split remaining rows.
        let flattened = crate::pdf_generator::text_support::flatten_math_environments(expression);
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
            if builder.unicode_font_encoder.is_some()
                && !use_base14_normalization()
                && let Some(enc) = &builder.unicode_font_encoder
            {
                return enc.estimate_width(text, size);
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
}
