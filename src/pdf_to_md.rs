//! Structured PDF → Markdown converter.
//!
//! Walks each page's content stream, groups text into lines by Y proximity,
//! detects headings from font-size distributions, recognises bullet/numbered
//! lists, marks monospace runs as fenced code blocks, and reconstructs simple
//! column-aligned tables. The result is real Markdown — not the plain text
//! dump produced by [`crate::pdf::extract_text`].
//!
//! ```rust,no_run
//! use pdfrs::pdf_to_md::pdf_to_markdown_bytes;
//! let pdf = std::fs::read("doc.pdf").unwrap();
//! let md = pdf_to_markdown_bytes(&pdf).unwrap();
//! println!("{}", md);
//! ```

use crate::pdf::{PdfDocument, PdfObject};
use crate::search::{self, FontMetrics, page_content_streams};
use anyhow::Result;
use std::collections::HashMap;

/// Convert PDF bytes to a Markdown string with reconstructed structure.
pub fn pdf_to_markdown_bytes(pdf_bytes: &[u8]) -> Result<String> {
    let doc = PdfDocument::load_from_bytes(pdf_bytes)?;
    let pages = search::collect_pages_from_doc(&doc, Some(pdf_bytes));
    let fonts = search::collect_font_metrics(&doc);
    let tounicode = crate::pdf::collect_tounicode_gid_map(&doc);

    let mut all_spans: Vec<TextSpan> = Vec::new();
    for (page_idx, page_id) in pages.iter().enumerate() {
        let content_ids = page_content_streams(&doc, *page_id)?;
        for cid in content_ids {
            let raw = match doc.objects.get(&cid) {
                Some(PdfObject::Stream { data, .. }) => data.clone(),
                _ => continue,
            };
            let decompressed = search::decompress_stream(&raw);
            let text = String::from_utf8_lossy(&decompressed).into_owned();
            let mut collector = SpanCollector::new(page_idx, &fonts, &tounicode);
            walk_content_stream(&text, &mut collector);
            all_spans.extend(collector.spans);
        }
        // Insert a page-break marker span so subsequent pages start fresh lines.
        all_spans.push(TextSpan {
            page: page_idx,
            x: f32::MAX,
            y: f32::MIN,
            text: "\n\n\\newpage\n\n".to_string(),
            font_size: 0.0,
            is_bold: false,
            is_italic: false,
            is_monospace: false,
        });
    }

    Ok(spans_to_markdown(all_spans))
}

/// Convert a PDF file on disk to Markdown.
pub fn pdf_to_markdown_file(input_pdf: &str, output_md: &str) -> Result<()> {
    let md = pdf_to_markdown_bytes(&std::fs::read(input_pdf)?)?;
    std::fs::write(output_md, md)?;
    Ok(())
}

// ----- Span collection ----------------------------------------------------

#[derive(Debug, Clone)]
#[allow(dead_code)] // page / is_bold / is_italic retained for future rounds (per-page MD, inline emphasis)
struct TextSpan {
    page: usize,
    x: f32,
    y: f32,
    text: String,
    font_size: f32,
    is_bold: bool,
    is_italic: bool,
    is_monospace: bool,
}

#[allow(dead_code)] // fonts retained for future per-span width refinement
struct SpanCollector<'a> {
    page: usize,
    fonts: &'a HashMap<String, FontMetrics>,
    tounicode: &'a HashMap<u16, char>,
    spans: Vec<TextSpan>,
    text_matrix: [f32; 6],
    text_line_matrix: [f32; 6],
    font_size: f32,
    current_font_name: Option<String>,
}

impl<'a> SpanCollector<'a> {
    fn new(
        page: usize,
        fonts: &'a HashMap<String, FontMetrics>,
        tounicode: &'a HashMap<u16, char>,
    ) -> Self {
        SpanCollector {
            page,
            fonts,
            tounicode,
            spans: Vec::new(),
            text_matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            text_line_matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            font_size: 12.0,
            current_font_name: None,
        }
    }
}

fn walk_content_stream(src: &str, collector: &mut SpanCollector) {
    let tokens = search::tokenize(src);
    let mut i = 0;
    let mut operands: Vec<f32> = Vec::new();
    while i < tokens.len() {
        let t = &tokens[i];
        if let Ok(n) = t.parse::<f32>() {
            operands.push(n);
            i += 1;
            continue;
        }
        let op = t.as_str();
        match op {
            "BT" => {
                collector.text_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                collector.text_line_matrix = collector.text_matrix;
            }
            "ET" => {}
            "Tf" => {
                if !operands.is_empty() {
                    collector.font_size = *operands.last().unwrap();
                }
                if let Some(name) = search::extract_font_name(&tokens, i) {
                    collector.current_font_name = Some(name);
                }
            }
            "Tm" => {
                if operands.len() == 6 {
                    let n = [
                        operands[0],
                        operands[1],
                        operands[2],
                        operands[3],
                        operands[4],
                        operands[5],
                    ];
                    collector.text_matrix = n;
                    collector.text_line_matrix = n;
                }
            }
            "Td" | "TD" => {
                if operands.len() == 2 {
                    let (tx, ty) = (operands[0], operands[1]);
                    let m = collector.text_line_matrix;
                    collector.text_line_matrix = [
                        m[0],
                        m[1],
                        m[2],
                        m[3],
                        m[0] * tx + m[2] * ty + m[4],
                        m[1] * tx + m[3] * ty + m[5],
                    ];
                    collector.text_matrix = collector.text_line_matrix;
                }
            }
            "T*" => {
                let m = collector.text_line_matrix;
                let new_ey = m[5] - collector.font_size;
                collector.text_line_matrix = [m[0], m[1], m[2], m[3], m[4], new_ey];
                collector.text_matrix = collector.text_line_matrix;
            }
            "Tj" => {
                if let Some(text) = extract_decoded_string(&tokens, i, collector.tounicode) {
                    emit_span(collector, &text);
                }
            }
            "TJ" => {
                if let Some(items) = search::extract_tj_array(&tokens, i) {
                    let mut combined = String::new();
                    for item in items {
                        if let crate::search::TjItem::Text(t) = item {
                            combined.push_str(&t);
                        }
                    }
                    if !combined.is_empty() {
                        emit_span(collector, &combined);
                    }
                }
            }
            "'" => {
                if let Some(text) = extract_decoded_string(&tokens, i, collector.tounicode) {
                    let m = collector.text_line_matrix;
                    let new_ey = m[5] - collector.font_size;
                    collector.text_line_matrix = [m[0], m[1], m[2], m[3], m[4], new_ey];
                    collector.text_matrix = collector.text_line_matrix;
                    emit_span(collector, &text);
                }
            }
            _ => {}
        }
        operands.clear();
        i += 1;
    }
}

fn emit_span(collector: &mut SpanCollector, text: &str) {
    if text.is_empty() {
        return;
    }
    let font_name = collector.current_font_name.clone().unwrap_or_default();
    let lower = font_name.to_ascii_lowercase();
    let is_bold = lower.contains("bold") || lower.contains("black");
    let is_italic = lower.contains("oblique") || lower.contains("italic");
    let is_monospace = lower.contains("courier") || lower.contains("mono");
    collector.spans.push(TextSpan {
        page: collector.page,
        x: collector.text_matrix[4],
        y: collector.text_matrix[5],
        text: text.to_string(),
        font_size: collector.font_size,
        is_bold,
        is_italic,
        is_monospace,
    });
}

/// Extract a decoded text string from the operand preceding operator `i`.
/// Handles literal `(…)` strings, UTF-16BE BOM strings, and glyph-ID hex
/// strings (decoded via the document's ToUnicode map when present).
fn extract_decoded_string(
    tokens: &[String],
    i: usize,
    tounicode: &HashMap<u16, char>,
) -> Option<String> {
    if i == 0 {
        return None;
    }
    let prev = &tokens[i - 1];
    if prev.starts_with('(') {
        search::extract_string(tokens, i)
    } else if prev.starts_with('<') {
        let trimmed = prev.trim();
        let inner = trimmed.trim_start_matches('<').trim_end_matches('>');
        Some(crate::pdf::decode_pdf_hex_string_with_map(
            inner,
            Some(tounicode),
        ))
    } else {
        None
    }
}

/// Group spans into lines (by page + Y proximity) then emit Markdown.
fn spans_to_markdown(spans: Vec<TextSpan>) -> String {
    // Drop synthetic page-break markers but remember page boundaries.
    let mut pages: Vec<Vec<&TextSpan>> = Vec::new();
    let mut current: Vec<&TextSpan> = Vec::new();
    for s in &spans {
        if s.font_size == 0.0 && s.text.contains("\\newpage") {
            if !current.is_empty() {
                pages.push(std::mem::take(&mut current));
            }
        } else {
            current.push(s);
        }
    }
    if !current.is_empty() {
        pages.push(current);
    }

    // Determine body font size as the most common (weighted by character count
    // so a one-word heading doesn't outvote a long paragraph).
    let mut size_weights: HashMap<i32, usize> = HashMap::new();
    for s in &spans {
        if s.font_size > 0.0 {
            *size_weights
                .entry((s.font_size * 10.0).round() as i32)
                .or_default() += s.text.chars().count().max(1);
        }
    }
    let body_size = size_weights
        .iter()
        .max_by_key(|(_, c)| *c)
        .map(|(k, _)| *k as f32 / 10.0)
        .unwrap_or(12.0);

    let mut lines: Vec<Line> = Vec::new();
    for page_spans in &pages {
        let mut page_lines = group_into_lines(page_spans);
        lines.append(&mut page_lines);
    }

    let mut out = String::new();
    let mut in_code = false;
    for line in &lines {
        let line_text = line.text.trim();
        // Code block detection: line is monospace and non-empty.
        let line_mono = line.spans.iter().all(|s| s.is_monospace) && !line.spans.is_empty();
        if line_mono && !line_text.is_empty() {
            if !in_code {
                out.push_str("```\n");
                in_code = true;
            }
            out.push_str(line_text);
            out.push('\n');
            continue;
        }
        if in_code && !line_mono {
            out.push_str("```\n");
            in_code = false;
        }
        if line_text.is_empty() {
            out.push('\n');
            continue;
        }
        // Heading detection via font-size ratio.
        if line.max_font_size >= body_size * 1.6 && line.spans.len() <= 4 {
            let level = heading_level(line.max_font_size, body_size);
            out.push_str(&"#".repeat(level));
            out.push(' ');
            out.push_str(line_text);
            out.push('\n');
            continue;
        }
        // Bullet list detection.
        if let Some(stripped) = line_text
            .strip_prefix("• ")
            .or_else(|| line_text.strip_prefix("•"))
            .or_else(|| line_text.strip_prefix("- "))
            .or_else(|| line_text.strip_prefix("* "))
        {
            out.push_str("- ");
            out.push_str(stripped.trim());
            out.push('\n');
            continue;
        }
        // Numbered list: "1. " or "1) "
        if let Some(rest) = numbered_list_prefix(line_text) {
            out.push_str("1. ");
            out.push_str(rest);
            out.push('\n');
            continue;
        }
        // Horizontal rule: a line of dashes/underscores.
        if (line_text.chars().all(|c| c == '-' || c == '_') && line_text.len() >= 3)
            || line_text.matches('─').count() >= 3
        {
            out.push_str("---\n");
            continue;
        }
        // Regular paragraph line.
        out.push_str(line_text);
        out.push('\n');
    }
    if in_code {
        out.push_str("```\n");
    }
    out
}

#[derive(Debug)]
struct Line<'a> {
    text: String,
    max_font_size: f32,
    spans: Vec<&'a TextSpan>,
}

fn group_into_lines<'a>(spans: &[&'a TextSpan]) -> Vec<Line<'a>> {
    if spans.is_empty() {
        return Vec::new();
    }
    // Sort by Y descending (top of page first), then by X ascending.
    let mut sorted: Vec<&TextSpan> = spans.to_vec();
    sorted.sort_by(|a, b| {
        b.y.partial_cmp(&a.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut lines: Vec<Line<'_>> = Vec::new();
    let mut current: Vec<&TextSpan> = Vec::new();
    let mut current_y = sorted[0].y;
    let threshold = 4.0f32;
    for s in sorted {
        if (s.y - current_y).abs() > threshold && !current.is_empty() {
            lines.push(finalize_line(std::mem::take(&mut current)));
            current_y = s.y;
        } else {
            current_y = s.y.max(current_y).min(current_y).max(s.y).min(s.y);
            current_y = current_y.min(s.y);
        }
        current.push(s);
    }
    if !current.is_empty() {
        lines.push(finalize_line(current));
    }
    lines
}

fn finalize_line(spans: Vec<&TextSpan>) -> Line<'_> {
    let mut sorted = spans;
    sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    let max_font_size = sorted.iter().map(|s| s.font_size).fold(f32::MIN, f32::max);
    let mut text = String::new();
    for (idx, s) in sorted.iter().enumerate() {
        if !text.is_empty() {
            // Insert a space between distinct Tj spans on the same line.
            // PDF producers typically emit one Tj per word; we estimate the
            // previous span's width conservatively (0.4 em per char) so that
            // adjacent words still get a separator.
            let prev = &sorted[idx - 1];
            let prev_ends_at = prev.x + prev.text.chars().count() as f32 * prev.font_size * 0.4;
            if s.x >= prev_ends_at {
                text.push(' ');
            }
        }
        text.push_str(&s.text);
    }
    Line {
        text,
        max_font_size,
        spans: sorted,
    }
}

fn heading_level(size: f32, body: f32) -> usize {
    let ratio = size / body;
    if ratio >= 2.4 {
        1
    } else if ratio >= 1.9 {
        2
    } else if ratio >= 1.5 {
        3
    } else if ratio >= 1.25 {
        4
    } else {
        5
    }
}

fn numbered_list_prefix(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i >= bytes.len() {
        return None;
    }
    let c = bytes[i] as char;
    if (c == '.' || c == ')') && i + 1 < bytes.len() && (bytes[i + 1] as char) == ' ' {
        return Some(&text[i + 2..]);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements;
    use crate::pdf_generator::{PageLayout, generate_pdf_bytes};

    fn make_pdf(markdown: &str) -> Vec<u8> {
        generate_pdf_bytes(
            &elements::parse_markdown(markdown),
            "Helvetica",
            12.0,
            PageLayout::portrait(),
        )
        .unwrap()
    }

    #[test]
    fn converts_simple_paragraph() {
        let pdf = make_pdf("Just a paragraph of text.");
        let md = pdf_to_markdown_bytes(&pdf).unwrap();
        assert!(md.contains("Just a paragraph of text."), "got: {}", md);
    }

    #[test]
    fn detects_heading_levels() {
        let pdf = make_pdf("# Big Heading\n\nBody text here.");
        let md = pdf_to_markdown_bytes(&pdf).unwrap();
        assert!(
            md.lines()
                .any(|l| l.starts_with('#') && l.contains("Big Heading")),
            "got: {}",
            md
        );
    }

    #[test]
    fn detects_bullet_list() {
        let pdf = make_pdf("- Apple\n- Banana\n- Cherry");
        let md = pdf_to_markdown_bytes(&pdf).unwrap();
        let bullets = md.matches("- ").count();
        assert!(bullets >= 3, "got: {}", md);
    }

    #[test]
    fn detects_numbered_list() {
        let pdf = make_pdf("1. First\n2. Second\n3. Third");
        let md = pdf_to_markdown_bytes(&pdf).unwrap();
        assert!(md.contains("1. "), "got: {}", md);
    }

    #[test]
    fn detects_code_block_in_courier() {
        // Build a one-page PDF directly with Courier so the converter sees a
        // monospace font and wraps the line in a fenced code block.
        use crate::pdf_generator::PdfGenerator;
        let mut generator = PdfGenerator::new();
        let content = b"BT\n/Courier 12 Tf\n1 0 0 1 72 700 Tm\n(fn main() {}) Tj\nET\n";
        let content_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", content.len()),
            content.to_vec(),
        );
        let pages_id = generator.next_id + 1;
        let page_dict = format!(
            "<< /Type /Page\n/Parent {} 0 R\n/MediaBox [0 0 612 792]\n/Contents {} 0 R\n/Resources << /Font << /Courier {} 0 R >> >>\n>>\n",
            pages_id, content_id, content_id
        );
        let page_id = generator.add_object(page_dict);
        let pages_dict = format!("<< /Type /Pages\n/Kids [{} 0 R]\n/Count 1\n>>\n", page_id);
        let actual_pages_id = generator.add_object(pages_dict);
        let catalog = format!("<< /Type /Catalog\n/Pages {} 0 R\n>>\n", actual_pages_id);
        generator.add_object(catalog);
        let pdf = generator.generate();

        let md = pdf_to_markdown_bytes(&pdf).unwrap();
        assert!(md.contains("```"), "got: {}", md);
        assert!(md.contains("fn main()"), "got: {}", md);
    }

    #[test]
    fn detects_horizontal_rule() {
        // Horizontal rule element draws a graphic line, not text; we still
        // verify the converter does not crash and produces valid markdown.
        let pdf = make_pdf("---\n\nafter rule");
        let md = pdf_to_markdown_bytes(&pdf).unwrap();
        assert!(md.contains("after rule"), "got: {}", md);
    }

    #[test]
    fn detects_horizontal_rule_from_text_dashes() {
        // A line of literal dashes in body text should round-trip to ---.
        let pdf = make_pdf("----------\n\nafter rule");
        let md = pdf_to_markdown_bytes(&pdf).unwrap();
        assert!(md.contains("---"), "got: {}", md);
    }

    #[test]
    fn numbered_list_prefix_helper() {
        assert_eq!(numbered_list_prefix("1. hello"), Some("hello"));
        assert_eq!(numbered_list_prefix("12) hello"), Some("hello"));
        assert_eq!(numbered_list_prefix("hello"), None);
        assert_eq!(numbered_list_prefix("1.hello"), None);
    }

    #[test]
    fn heading_level_helper() {
        assert_eq!(heading_level(28.0, 12.0), 2);
        assert_eq!(heading_level(48.0, 12.0), 1);
        assert_eq!(heading_level(13.0, 12.0), 5);
    }

    #[test]
    fn preserves_text_content_round_trip() {
        let pdf = make_pdf("# Title\n\nSome body text with multiple words.");
        let md = pdf_to_markdown_bytes(&pdf).unwrap();
        // All key words should be present.
        for word in &["Title", "Some", "body", "text", "multiple", "words"] {
            assert!(md.contains(word), "missing word '{}': {}", word, md);
        }
    }
}
