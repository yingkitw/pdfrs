//! Content-stream walker, hit collection, TJ extraction, tokenization.
use super::metrics::FontMetrics;
use super::pages::{Rect, SearchHit};
use std::collections::HashMap;

// ----- Content-stream walking --------------------------------------------

/// Receives text spans as the walker encounters them.
pub(super) struct HitCollector<'a> {
    page: usize,
    needle: &'a str,
    fonts: &'a HashMap<String, FontMetrics>,
    case_insensitive: bool,
    /// Pending text/position info; flushed on line break or ET.
    buffer: String,
    span_start_x: f32,
    span_baseline_y: f32,
    span_font_size: f32,
    /// Per-character bounding boxes, parallel to `buffer.chars()`.
    boxes: Vec<(f32, f32, f32)>, // (x, baseline_y, advance_pt)
    hits: Vec<SearchHit>,
    /// Tracks the most recent text line's `Tm` so position errors are bounded.
    last_line_y: f32,
    /// Font metrics for the current Tf.
    current_metrics: Option<FontMetrics>,
}

impl<'a> HitCollector<'a> {
    pub(super) fn new(
        page: usize,
        needle: &'a str,
        fonts: &'a HashMap<String, FontMetrics>,
        case_insensitive: bool,
    ) -> Self {
        HitCollector {
            page,
            needle,
            fonts,
            case_insensitive,
            buffer: String::new(),
            span_start_x: 0.0,
            span_baseline_y: 0.0,
            span_font_size: 12.0,
            boxes: Vec::new(),
            hits: Vec::new(),
            last_line_y: f32::MAX,
            current_metrics: None,
        }
    }

    pub(super) fn into_hits(mut self) -> Vec<SearchHit> {
        self.flush_buffer();
        self.hits
    }

    fn push_text(&mut self, text: &str, x: f32, baseline_y: f32, font_size: f32) {
        // Flush if we've moved to a new line.
        if (self.last_line_y - baseline_y).abs() > 2.0 && !self.buffer.is_empty() {
            self.flush_buffer();
        }
        if self.buffer.is_empty() {
            self.span_start_x = x;
            self.span_baseline_y = baseline_y;
            self.span_font_size = font_size;
        }
        self.last_line_y = baseline_y;
        let mut cursor_x = if self.buffer.is_empty() {
            x
        } else {
            // Continue from where the last char left off.
            self.boxes.last().map(|b| b.0 + b.2).unwrap_or(x)
        };
        let metrics = self.current_metrics.clone();
        for ch in text.chars() {
            let cp = ch as u32;
            let advance_units = metrics.as_ref().map(|m| m.advance(cp)).unwrap_or(500);
            let advance_pt = advance_units as f32 * font_size / 1000.0;
            self.buffer.push(ch);
            self.boxes.push((cursor_x, baseline_y, advance_pt));
            cursor_x += advance_pt;
        }
    }

    fn flush_buffer(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let haystack = if self.case_insensitive {
            self.buffer.to_lowercase()
        } else {
            self.buffer.clone()
        };
        let mut search_from = 0usize;
        while let Some(rel) = find_subslice(&haystack[search_from..], self.needle) {
            let abs_start = search_from + rel;
            let abs_end = abs_start + self.needle.chars().count();
            // Compute bbox covering the matched chars.
            let start = self.boxes.get(abs_start).copied();
            let end = self.boxes.get(abs_end.saturating_sub(1)).copied();
            if let (Some((sx, by, _)), Some((ex, _, eadv))) = (start, end) {
                let bbox = Rect {
                    x: sx,
                    y: by - self.span_font_size,
                    width: (ex + eadv) - sx,
                    height: self.span_font_size,
                };
                let matched: String = self
                    .buffer
                    .chars()
                    .skip(abs_start)
                    .take(abs_end - abs_start)
                    .collect();
                let snippet = make_snippet(&self.buffer, abs_start, abs_end, 24);
                self.hits.push(SearchHit {
                    page: self.page,
                    text: matched,
                    snippet,
                    bbox,
                });
            }
            search_from = abs_start + self.needle.chars().count().max(1);
        }
        self.buffer.clear();
        self.boxes.clear();
    }

    fn set_font(&mut self, name: &str) {
        self.current_metrics = self.fonts.get(name).cloned();
    }
}

fn find_subslice(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    // Operate on char indices to handle multi-byte chars safely.
    let hchars: Vec<char> = haystack.chars().collect();
    let nchars: Vec<char> = needle.chars().collect();
    if nchars.len() > hchars.len() {
        return None;
    }
    for i in 0..=(hchars.len() - nchars.len()) {
        if hchars[i..i + nchars.len()] == nchars[..] {
            return Some(i);
        }
    }
    None
}

fn make_snippet(buffer: &str, start: usize, end: usize, pad: usize) -> String {
    let chars: Vec<char> = buffer.chars().collect();
    let total = chars.len();
    let lo = start.saturating_sub(pad);
    let hi = (end + pad).min(total);
    let prefix = if lo > 0 { "…" } else { "" };
    let suffix = if hi < total { "…" } else { "" };
    let middle: String = chars[lo..hi].iter().collect();
    format!("{prefix}{middle}{suffix}")
}

pub(super) fn walk_content_stream(
    src: &str,
    collector: &mut HitCollector,
    tounicode: Option<&HashMap<u16, char>>,
) {
    let tokens = tokenize(src);
    let mut i = 0;
    let mut operands: Vec<f32> = Vec::new();
    let mut text_matrix = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut text_line_matrix = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut font_size = 12.0f32;
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
                text_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                text_line_matrix = text_matrix;
                collector.flush_buffer();
            }
            "ET" => {
                collector.flush_buffer();
            }
            "Tf" => {
                if operands.len() >= 2 {
                    font_size = *operands.last().unwrap();
                }
                if let Some(name) = extract_font_name(&tokens, i) {
                    collector.set_font(&name);
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
                    text_matrix = n;
                    text_line_matrix = n;
                    collector.flush_buffer();
                }
            }
            "Td" => {
                if operands.len() == 2 {
                    let (tx, ty) = (operands[0], operands[1]);
                    let m = text_line_matrix;
                    text_line_matrix = [
                        m[0],
                        m[1],
                        m[2],
                        m[3],
                        m[0] * tx + m[2] * ty + m[4],
                        m[1] * tx + m[3] * ty + m[5],
                    ];
                    text_matrix = text_line_matrix;
                    collector.flush_buffer();
                }
            }
            "TD" => {
                if operands.len() == 2 {
                    let (tx, ty) = (operands[0], operands[1]);
                    let m = text_line_matrix;
                    text_line_matrix = [
                        m[0],
                        m[1],
                        m[2],
                        m[3],
                        m[0] * tx + m[2] * ty + m[4],
                        m[1] * tx + m[3] * ty + m[5],
                    ];
                    text_matrix = text_line_matrix;
                    collector.flush_buffer();
                }
            }
            "T*" => {
                let m = text_line_matrix;
                let new_ey = m[5] - font_size;
                text_line_matrix = [m[0], m[1], m[2], m[3], m[4], new_ey];
                text_matrix = text_line_matrix;
                collector.flush_buffer();
            }
            "Tj" => {
                if let Some(text) = extract_string_with_map(&tokens, i, tounicode) {
                    let x = text_matrix[4];
                    let y = text_matrix[5];
                    collector.push_text(&text, x, y, font_size);
                    let advance = collector.boxes.last().map(|b| b.2).unwrap_or(0.0);
                    text_matrix[4] = x + advance;
                }
            }
            "TJ" => {
                if let Some(items) = extract_tj_array_with_map(&tokens, i, tounicode) {
                    let mut x = text_matrix[4];
                    let y = text_matrix[5];
                    for item in items {
                        match item {
                            TjItem::Text(t) => {
                                collector.push_text(&t, x, y, font_size);
                                x = collector.boxes.last().map(|b| b.0 + b.2).unwrap_or(x);
                            }
                            TjItem::Kern(amount) => {
                                // Negative numbers advance left; positives gap.
                                x += amount;
                            }
                        }
                    }
                    text_matrix[4] = x;
                }
            }
            "'" => {
                if let Some(text) = extract_string_with_map(&tokens, i, tounicode) {
                    let m = text_line_matrix;
                    let new_ey = m[5] - font_size;
                    text_line_matrix = [m[0], m[1], m[2], m[3], m[4], new_ey];
                    text_matrix = text_line_matrix;
                    let x = text_matrix[4];
                    let y = text_matrix[5];
                    collector.push_text(&text, x, y, font_size);
                }
            }
            _ => {}
        }
        operands.clear();
        i += 1;
    }
    collector.flush_buffer();
}

#[derive(Debug)]
pub(crate) enum TjItem {
    Text(String),
    Kern(f32),
}

pub(crate) fn extract_tj_array(tokens: &[String], i: usize) -> Option<Vec<TjItem>> {
    extract_tj_array_with_map(tokens, i, None)
}

pub(crate) fn extract_tj_array_with_map(
    tokens: &[String],
    i: usize,
    tounicode: Option<&HashMap<u16, char>>,
) -> Option<Vec<TjItem>> {
    if i == 0 {
        return None;
    }
    let prev = &tokens[i - 1];
    if !prev.starts_with('[') || !prev.ends_with(']') {
        return None;
    }
    let mut out = Vec::new();
    let mut k = 0;
    let bytes = prev.as_bytes();
    while k < bytes.len() {
        let c = bytes[k] as char;
        if c == '(' {
            let start = k + 1;
            let mut end = start;
            let mut depth = 1;
            while end < bytes.len() && depth > 0 {
                let cc = bytes[end] as char;
                if cc == '\\' && end + 1 < bytes.len() {
                    end += 2;
                    continue;
                }
                if cc == '(' {
                    depth += 1;
                } else if cc == ')' {
                    depth -= 1;
                }
                if depth > 0 {
                    end += 1;
                }
            }
            if let Some(text) = parse_pdf_literal_string(&prev[start..end + 1]) {
                out.push(TjItem::Text(text));
            }
            k = end + 1;
        } else if c == '<' {
            let start = k + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end] as char) != '>' {
                end += 1;
            }
            let decoded = crate::pdf::decode_pdf_hex_string_with_map(&prev[start..end], tounicode);
            out.push(TjItem::Text(decoded));
            k = end + 1;
        } else if c == ']' {
            break;
        } else {
            // Number or whitespace — accumulate numeric token.
            let start = k;
            while k < bytes.len() {
                let cc = bytes[k] as char;
                if cc.is_ascii_digit() || cc == '.' || cc == '-' || cc == '+' {
                    k += 1;
                } else {
                    break;
                }
            }
            if k > start
                && let Some(num) = prev[start..k].parse::<f32>().ok()
            {
                out.push(TjItem::Kern(num));
            } else {
                k += 1;
            }
        }
    }
    Some(out)
}

pub(crate) fn extract_font_name(tokens: &[String], i: usize) -> Option<String> {
    // `Tf` operands are `/FontName size`. The font name is the operand that
    // starts with `/` and appears immediately before the size operand.
    if i < 2 {
        return None;
    }
    // Walk backwards from i-1 looking for a token that starts with '/'.
    let mut j = i;
    while j > 0 {
        j -= 1;
        let t = &tokens[j];
        if let Some(stripped) = t.strip_prefix('/') {
            return Some(stripped.to_string());
        }
        // Stop at the previous operator (alphabetic token without '/').
        if t.chars().all(|c| c.is_ascii_alphabetic()) && !t.is_empty() {
            break;
        }
    }
    None
}

pub(crate) fn extract_string(tokens: &[String], i: usize) -> Option<String> {
    extract_string_with_map(tokens, i, None)
}

pub(crate) fn extract_string_with_map(
    tokens: &[String],
    i: usize,
    tounicode: Option<&HashMap<u16, char>>,
) -> Option<String> {
    if i == 0 {
        return None;
    }
    let prev = &tokens[i - 1];
    if prev.starts_with('(') {
        parse_pdf_literal_string(prev)
    } else if prev.starts_with('<') {
        // Hex string — decode bytes then handle UTF-16BE BOM or glyph IDs.
        let trimmed = prev.trim();
        let inner = trimmed.trim_start_matches('<').trim_end_matches('>');
        Some(crate::pdf::decode_pdf_hex_string_with_map(inner, tounicode))
    } else {
        None
    }
}

fn parse_pdf_literal_string(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut k = 0;
    while k < bytes.len() {
        let b = bytes[k];
        if b == b'\\' && k + 1 < bytes.len() {
            let nxt = bytes[k + 1];
            match nxt {
                b'n' => {
                    out.push(b'\n');
                    k += 2;
                    continue;
                }
                b'r' => {
                    out.push(b'\r');
                    k += 2;
                    continue;
                }
                b't' => {
                    out.push(b'\t');
                    k += 2;
                    continue;
                }
                b'\\' | b'(' | b')' => {
                    out.push(nxt);
                    k += 2;
                    continue;
                }
                d if d.is_ascii_digit() => {
                    let mut oct = String::new();
                    oct.push(d as char);
                    let mut j = k + 2;
                    while j < bytes.len() && oct.len() < 3 && (bytes[j] as char).is_ascii_digit() {
                        oct.push(bytes[j] as char);
                        j += 1;
                    }
                    if let Ok(v) = u8::from_str_radix(&oct, 8) {
                        out.push(v);
                    }
                    k = j;
                    continue;
                }
                _ => {}
            }
        }
        if out.is_empty() && b == 0xFE && k + 1 < bytes.len() && bytes[k + 1] == 0xFF {
            k += 2;
            while k + 1 < bytes.len() {
                let cu = u16::from_be_bytes([bytes[k], bytes[k + 1]]);
                if let Some(c) = char::from_u32(cu as u32) {
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    out.extend_from_slice(s.as_bytes());
                }
                k += 2;
            }
            return Some(String::from_utf8_lossy(&out).into_owned());
        }
        out.push(b);
        k += 1;
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

// ----- Tokenizer ----------------------------------------------------------

pub(crate) fn tokenize(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '%' => {
                while i < bytes.len() && (bytes[i] as char) != '\n' {
                    i += 1;
                }
            }
            '(' => {
                let start = i;
                let mut depth = 1;
                i += 1;
                while i < bytes.len() && depth > 0 {
                    let cc = bytes[i] as char;
                    if cc == '\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if cc == '(' {
                        depth += 1;
                    } else if cc == ')' {
                        depth -= 1;
                    }
                    i += 1;
                }
                out.push(src[start..i].to_string());
            }
            '<' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'<' {
                    out.push("<<".to_string());
                    i += 2;
                } else {
                    let start = i;
                    i += 1;
                    while i < bytes.len() && (bytes[i] as char) != '>' {
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1;
                    }
                    out.push(src[start..i].to_string());
                }
            }
            '>' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                    out.push(">>".to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            '[' => {
                let start = i;
                let mut depth = 1;
                i += 1;
                while i < bytes.len() && depth > 0 {
                    let cc = bytes[i] as char;
                    if cc == '\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if cc == '[' {
                        depth += 1;
                    } else if cc == ']' {
                        depth -= 1;
                    }
                    i += 1;
                }
                out.push(src[start..i].to_string());
            }
            ']' => {
                i += 1;
            }
            '/' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    let cc = bytes[i] as char;
                    if cc.is_whitespace() || "/<>()[]{}".contains(cc) {
                        break;
                    }
                    i += 1;
                }
                out.push(src[start..i].to_string());
            }
            _ if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' => {
                let start = i;
                if c == '-' || c == '+' {
                    i += 1;
                }
                while i < bytes.len() {
                    let cc = bytes[i] as char;
                    if cc.is_ascii_digit() || cc == '.' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push(src[start..i].to_string());
            }
            _ if c.is_ascii_alphabetic() => {
                let start = i;
                while i < bytes.len() {
                    let cc = bytes[i] as char;
                    if cc.is_ascii_alphabetic() || cc == '*' || cc == '\'' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push(src[start..i].to_string());
            }
            _ => {
                i += 1;
            }
        }
    }
    out
}
