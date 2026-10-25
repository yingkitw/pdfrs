//! Tokenisation and operand extraction for content streams.

use crate::raster::surface::Color;

/// Read a `/Name` operand that directly precedes the operator at `i`.
pub(super) fn extract_name_before(tokens: &[String], i: usize) -> Option<String> {
    if i == 0 {
        return None;
    }
    tokens[i - 1].strip_prefix('/').map(|s| s.to_string())
}

pub(super) fn extract_font_name(tokens: &[String], i: usize) -> Option<String> {
    if i < 2 {
        return None;
    }
    // `Tf` operands are `/Name size`; the name is two tokens before the operator.
    let prev = &tokens[i - 2];
    if let Some(stripped) = prev.strip_prefix('/') {
        return Some(stripped.to_string());
    }
    None
}

/// Extract raw bytes from the string operand preceding operator at index `i`.
/// For hex strings `<...>`, returns the decoded bytes. For literal strings `(...)`,
/// returns the decoded byte content.
pub(super) fn extract_string_raw(tokens: &[String], i: usize) -> Option<Vec<u8>> {
    if i == 0 {
        return None;
    }
    let prev = &tokens[i - 1];
    let trimmed = prev.trim();
    if trimmed.starts_with('<') && trimmed.ends_with('>') {
        // Hex string
        let hex: String = trimmed[1..trimmed.len() - 1]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .filter_map(|j| {
                if j + 1 < hex.len() {
                    u8::from_str_radix(&hex[j..j + 2], 16).ok()
                } else if j < hex.len() {
                    u8::from_str_radix(&hex[j..j + 1], 16).ok()
                } else {
                    None
                }
            })
            .collect();
        Some(bytes)
    } else if trimmed.starts_with('(') && trimmed.ends_with(')') {
        // Literal string — decode escapes and return bytes
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
                        while j < bytes.len()
                            && oct.len() < 3
                            && (bytes[j] as char).is_ascii_digit()
                        {
                            oct.push(bytes[j] as char);
                            j += 1;
                        }
                        if let Ok(v) = u8::from_str_radix(&oct, 8) {
                            out.push(v);
                        }
                        k = j;
                        continue;
                    }
                    _ => {
                        out.push(b);
                        k += 1;
                        continue;
                    }
                }
            }
            out.push(b);
            k += 1;
        }
        Some(out)
    } else {
        None
    }
}

/// Extract raw bytes from a TJ array operand, concatenating all string elements
/// (both hex `<...>` and literal `(...)` strings) and skipping numeric kerning values.
pub(super) fn extract_array_strings_raw(tokens: &[String], i: usize) -> Option<Vec<u8>> {
    if i == 0 {
        return None;
    }
    let prev = &tokens[i - 1];
    if !prev.starts_with('[') || !prev.ends_with(']') {
        return None;
    }
    let mut out = Vec::new();
    let bytes = prev.as_bytes();
    let mut k = 0;
    while k < bytes.len() {
        let c = bytes[k] as char;
        if c == '(' {
            let start = k + 1;
            let mut end = start;
            let mut d = 1;
            while end < bytes.len() && d > 0 {
                let cc = bytes[end] as char;
                if cc == '\\' && end + 1 < bytes.len() {
                    end += 2;
                    continue;
                }
                if cc == '(' {
                    d += 1;
                } else if cc == ')' {
                    d -= 1;
                }
                if d > 0 {
                    end += 1;
                }
            }
            // Decode literal string bytes
            let inner = &prev[start..end];
            if let Some(s) = parse_pdf_literal_string(&format!("({})", inner)) {
                out.extend_from_slice(s.as_bytes());
            }
            k = end + 1;
        } else if c == '<' {
            let start = k + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] as char != '>' {
                end += 1;
            }
            // Decode hex string bytes
            let hex: String = prev[start..end]
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect();
            for j in (0..hex.len()).step_by(2) {
                if j + 1 < hex.len() {
                    if let Ok(v) = u8::from_str_radix(&hex[j..j + 2], 16) {
                        out.push(v);
                    }
                } else if j < hex.len()
                    && let Ok(v) = u8::from_str_radix(&hex[j..j + 1], 16)
                {
                    out.push(v);
                }
            }
            k = end + 1;
        } else {
            k += 1;
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

pub(super) fn parse_pdf_literal_string(raw: &str) -> Option<String> {
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
        // UTF-16BE BOM
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

pub(super) fn parse_number(s: &str) -> Option<f32> {
    s.parse::<f32>().ok()
}

pub(super) fn cmyk_to_rgb(c: f32, m: f32, y: f32, k: f32) -> Color {
    let r = (1.0 - c) * (1.0 - k);
    let g = (1.0 - m) * (1.0 - k);
    let b = (1.0 - y) * (1.0 - k);
    Color::from_rgb(r, g, b)
}

// ----- Tokenizer ----------------------------------------------------------

pub(super) fn tokenize(src: &str) -> Vec<String> {
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
                // Could be hex string << dict or <...>
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
                // Arrays are typically emitted on a single line; capture until matching ']'.
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
