//! Element-to-builder render loop: dispatches each [`Element`] to its
//! `ContentStreamBuilder` emission method, with table-row accumulation.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::elements::Element;
use crate::pdf_generator::code_highlight::highlight_code;
use crate::pdf_generator::layout::{Color, PageLayout, TextAlign, heading_font_size, line_height};
use crate::pdf_generator::text_support::render_math_text;
use crate::pdf_generator::unicode_support::UnicodeFontEncoder;
use crate::thesis::{build_bibliography_elements, collect_citation_defs, expand_toc};

use super::ContentStreamBuilder;

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
    let mut table_colspans: Vec<Vec<u32>> = Vec::new();
    let mut table_rowspans: Vec<Vec<u32>> = Vec::new();

    for elem in elements {
        // Handle table rows specially - accumulate them
        if let Element::TableRow {
            cells,
            is_separator,
            alignments,
            colspans,
            rowspans,
        } = elem
        {
            if *is_separator {
                // Store alignments from separator row
                table_alignments = Some(alignments.clone());
            } else {
                // Only add non-separator rows to the table
                table_rows.push(cells.clone());
                table_colspans.push(colspans.clone());
                table_rowspans.push(rowspans.clone());
            }
            continue;
        }

        // Flush any accumulated table before rendering non-table element
        if !table_rows.is_empty() {
            builder.render_table(
                &table_rows,
                base_font_size,
                table_alignments.as_deref(),
                &table_colspans,
                &table_rowspans,
            );
            table_rows.clear();
            table_alignments = None;
            table_colspans.clear();
            table_rowspans.clear();
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
                    let rect_height = chunk_height + line_h;
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
                // Code-block drawing leaves the BT open from the last draw_line's
                // BT re-entry; close it so the next element starts a fresh text
                // block. Without this, the next heading/paragraph inherits a stale
                // text matrix and renders as a ghosted double-stamp.
                builder.end_text_block();
                builder.current.extend_from_slice(b"BT\n");
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
                series,
            } => {
                builder.emit_chart(*kind, title, points, series);
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
        builder.render_table(
            &table_rows,
            base_font_size,
            table_alignments.as_deref(),
            &table_colspans,
            &table_rowspans,
        );
    }
}
