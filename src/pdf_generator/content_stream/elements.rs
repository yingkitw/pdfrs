//! Element rendering: tables, wrapped text, rich paragraphs, and images.

use std::path::{Path, PathBuf};

use crate::elements::TextSegment;
use crate::image;
use crate::table_renderer::{PdfTableHelper, TableStyle};

use super::ContentStreamBuilder;
use crate::pdf_generator::layout::{
    Color, TextAlign, estimated_text_width, line_height, split_long_word_for_wrap,
};
use crate::pdf_generator::text_support::{render_math_text, use_base14_normalization};

impl ContentStreamBuilder {
    /// Render a complete table with borders, text wrapping, and alignment
    pub(super) fn render_table(
        &mut self,
        rows: &[Vec<String>],
        base_font_size: f32,
        alignments: Option<&[crate::elements::TableAlignment]>,
        colspans: &[Vec<u32>],
        rowspans: &[Vec<u32>],
    ) {
        if rows.is_empty() {
            return;
        }

        let table_helper = PdfTableHelper::default();
        let style = TableStyle::default();

        // Convert string rows to TableRow with alignments and spans
        let table_rows = table_helper.convert_rows(rows, alignments, colspans, rowspans);

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

        // Track cells occupied by rowspan from above so we skip them.
        // occupied[row][col] = true means a rowspan cell from above covers this position.
        let mut occupied: Vec<Vec<bool>> = (0..dims.num_rows)
            .map(|_| vec![false; dims.num_cols])
            .collect();

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
            let mut col = 0usize;
            for cell in &row.cells {
                if col >= dims.num_cols {
                    break;
                }
                // Skip cells occupied by a rowspan from above
                while col < dims.num_cols && occupied[row_idx][col] {
                    col_x += dims.column_widths[col];
                    col += 1;
                }
                if col >= dims.num_cols {
                    break;
                }

                let cs = cell.colspan.max(1) as usize;
                let rs = cell.rowspan.max(1) as usize;
                let pad_h = cell.padding_h.unwrap_or(style.cell_padding_h);

                // Cell width = sum of spanned column widths
                let cell_width: f32 = (col..col + cs)
                    .take_while(|&i| i < dims.num_cols)
                    .map(|i| dims.column_widths[i])
                    .sum();
                // Cell height = sum of spanned row heights
                let cell_height: f32 = (row_idx..row_idx + rs)
                    .take_while(|&i| i < dims.num_rows)
                    .map(|i| dims.row_heights[i])
                    .sum();

                let max_chars = ((cell_width - pad_h * 2.0) / approx_char_width)
                    .floor()
                    .max(1.0) as usize;

                // Wrap text into lines using the table helper
                let wrapped = table_helper.renderer().wrap_text(&cell.content, max_chars);

                // Calculate vertical centering
                let text_height = wrapped.line_count as f32 * line_h;
                let start_y_pos = row_y - (cell_height - text_height) / 2.0 - line_h / 3.0;

                // Render each line with proper alignment
                let last_line = wrapped.line_count.saturating_sub(1);
                for (line_idx, line) in wrapped.lines.iter().enumerate() {
                    let line_width = self.estimate_text_width(line, base_font_size);

                    // For Justify alignment, word-space all lines except the last
                    if cell.alignment == crate::elements::TableAlignment::Justify
                        && line_idx != last_line
                    {
                        // Render word by word with extra spacing
                        let words: Vec<&str> = line.split_whitespace().collect();
                        if words.len() > 1 {
                            let total_word_width: f32 = words
                                .iter()
                                .map(|w| self.estimate_text_width(w, base_font_size))
                                .sum();
                            let gap = (cell_width - pad_h * 2.0 - total_word_width)
                                / (words.len() - 1) as f32;
                            let mut word_x = col_x + pad_h;
                            let y = start_y_pos - (line_idx as f32 * line_h);
                            for word in &words {
                                self.current.extend_from_slice(
                                    format!("1 0 0 1 {} {} Tm\n", word_x, y).as_bytes(),
                                );
                                self.current.extend_from_slice(
                                    format!("{} Tj\n", self.encode_text_for_current_font(word))
                                        .as_bytes(),
                                );
                                word_x += self.estimate_text_width(word, base_font_size) + gap;
                            }
                            continue;
                        }
                    }

                    // Calculate X position using the table helper
                    let x = table_helper.renderer().calculate_text_x(
                        &cell.alignment,
                        col_x,
                        cell_width,
                        line_width,
                        pad_h,
                    );

                    let y = start_y_pos - (line_idx as f32 * line_h);

                    self.current
                        .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, y).as_bytes());
                    self.current.extend_from_slice(
                        format!("{} Tj\n", self.encode_text_for_current_font(line)).as_bytes(),
                    );
                }

                // Mark cells occupied by rowspan
                if rs > 1 {
                    for r in 1..rs {
                        for c in 0..cs {
                            let ri = row_idx + r;
                            let ci = col + c;
                            if ri < dims.num_rows && ci < dims.num_cols {
                                occupied[ri][ci] = true;
                            }
                        }
                    }
                }

                col_x += cell_width;
                col += cs;
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

    /// Emit wrapped text that fits within the content width
    pub(super) fn emit_wrapped_text(&mut self, text: &str, font_size: f32) {
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
    pub(super) fn emit_rich_paragraph(&mut self, segments: &[TextSegment], font_size: f32) {
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
    pub(super) fn emit_image(&mut self, alt: &str, path: &str) {
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
}
