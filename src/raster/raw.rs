//! Raw PDF byte scanning: object lookup, stream spans, and a minimal value parser.
//!
//! The crate's general-purpose dictionary parser is whitespace-tokenised and
//! therefore flattens nested dictionaries (`/Resources << /XObject << ... >> >>`
//! degrades to a flat map). The rasterizer therefore resolves the resources it
//! needs — `/ExtGState` and `/XObject` — by scanning the original PDF bytes
//! with the small parser below, which understands nesting properly.
//!
//! Everything here works on the **original** bytes: stream payloads must not
//! be round-tripped through UTF-8, because binary image data would be
//! corrupted by lossy decoding.

/// A value parsed out of a raw PDF dictionary or array.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum RawValue {
    /// A `/Name` token (without the leading slash).
    Name(String),
    /// A numeric literal.
    Number(f64),
    /// An indirect reference `N G R` (only the object number is kept).
    Reference(u32),
    /// An array of values.
    Array(Vec<RawValue>),
    /// A dictionary, preserving insertion order.
    Dict(Vec<(String, RawValue)>),
    /// A string literal or hex string, with its decoded bytes.
    Bytes(Vec<u8>),
    /// Anything else (booleans, `null`, streams).
    Other,
}

impl RawValue {
    /// Look up a key in a dictionary value.
    pub(super) fn get(&self, key: &str) -> Option<&RawValue> {
        match self {
            RawValue::Dict(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The object number of a reference value.
    pub(super) fn as_reference(&self) -> Option<u32> {
        match self {
            RawValue::Reference(id) => Some(*id),
            _ => None,
        }
    }

    /// The numeric value.
    pub(super) fn as_number(&self) -> Option<f64> {
        match self {
            RawValue::Number(n) => Some(*n),
            _ => None,
        }
    }
}

/// Locate the body of object `id` (between `N G obj` and `endobj`) in the
/// original PDF bytes.
///
/// The object number must start at a token boundary, so `12 0 obj` is not
/// mistaken for object `2`.
pub(super) fn find_object_body(bytes: &[u8], id: u32) -> Option<&[u8]> {
    let (start, end) = find_object_span(bytes, id)?;
    Some(&bytes[start..end])
}

/// Locate the byte range of object `id`'s body, excluding the `N G obj` header
/// and the trailing `endobj` keyword.
pub(super) fn find_object_span(bytes: &[u8], id: u32) -> Option<(usize, usize)> {
    let needle = format!("{id} ");
    let mut search_from = 0usize;
    while let Some(rel) = find_sub(bytes, needle.as_bytes(), search_from) {
        let abs = rel;
        // Must start at a token boundary.
        if abs > 0 {
            let prev = bytes[abs - 1];
            if prev.is_ascii_alphanumeric() || prev == b'.' {
                search_from = abs + 1;
                continue;
            }
        }
        let after = &bytes[abs + needle.len()..];
        // Expect `<gen> obj`.
        let mut i = 0;
        while i < after.len() && after[i].is_ascii_digit() {
            i += 1;
        }
        if i == 0 {
            search_from = abs + 1;
            continue;
        }
        while i < after.len() && (after[i] as char).is_whitespace() {
            i += 1;
        }
        if after.len() - i >= 3 && &after[i..i + 3] == b"obj" {
            let body_start = abs + needle.len() + i + 3;
            let end_rel = find_sub(bytes, b"endobj", body_start)?;
            return Some((body_start, end_rel));
        }
        search_from = abs + 1;
    }
    None
}

/// Locate the byte range of an object's **stream payload** in the original
/// bytes.
///
/// Per PDF 32000-1 §7.3.8.1 the payload starts after the `stream` keyword and
/// its EOL, and ends immediately before the EOL that precedes `endstream`.
/// Only that single EOL is removed — trailing whitespace inside the payload
/// is preserved, which matters for binary image data.
pub(super) fn find_stream_span(bytes: &[u8], id: u32) -> Option<(usize, usize)> {
    let (body_start, body_end) = find_object_span(bytes, id)?;
    let body = &bytes[body_start..body_end];
    let kw_rel = find_sub(body, b"stream", 0)?;
    // `stream` must not be the `endstream` keyword.
    let after_kw = &body[kw_rel + 6..];
    if after_kw.first().is_some_and(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let mut data_start = body_start + kw_rel + 6;
    // Skip the EOL after the keyword: either "\r\n" or "\n".
    if bytes.get(data_start) == Some(&b'\r') {
        data_start += 1;
    }
    if bytes.get(data_start) == Some(&b'\n') {
        data_start += 1;
    } else if data_start == body_start + kw_rel + 6 {
        // Some producers omit the EOL entirely.
    }
    let end_rel = find_sub(bytes, b"endstream", data_start)?;
    let mut data_end = end_rel;
    // Strip the single EOL that precedes `endstream`.
    if data_end > data_start && bytes[data_end - 1] == b'\n' {
        data_end -= 1;
    }
    if data_end > data_start && bytes[data_end - 1] == b'\r' {
        data_end -= 1;
    }
    Some((data_start, data_end))
}

/// Find `needle` in `haystack` at or after `from`.
pub(super) fn find_sub(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= haystack.len() {
        return None;
    }
    // The slice must reach the end of the buffer: `windows` then yields every
    // position where the needle could still fit.
    let end = haystack.len() - 1;
    if from > end {
        return None;
    }
    haystack[from..=end]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Parse a dictionary that starts with `<<` at `pos`.
///
/// Returns the entries and the position just past the closing `>>`.
pub(super) fn parse_dict_at(bytes: &[u8], pos: usize) -> Option<(Vec<(String, RawValue)>, usize)> {
    if bytes.get(pos) != Some(&b'<') || bytes.get(pos + 1) != Some(&b'<') {
        return None;
    }
    let mut i = pos + 2;
    let mut entries: Vec<(String, RawValue)> = Vec::new();
    loop {
        i = skip_ws(bytes, i);
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == b'>' {
            i += if bytes.get(i + 1) == Some(&b'>') {
                2
            } else {
                1
            };
            break;
        }
        if bytes[i] != b'/' {
            // Not a key: skip one value and continue.
            let before = i;
            parse_value_at(bytes, &mut i);
            if i == before {
                i += 1;
            }
            continue;
        }
        let Some(RawValue::Name(key)) = parse_value_at(bytes, &mut i) else {
            i += 1;
            continue;
        };
        let value = parse_value_at(bytes, &mut i).unwrap_or(RawValue::Other);
        entries.push((key, value));
    }
    Some((entries, i))
}

/// Parse a single value at `pos`, advancing `pos` past it.
pub(super) fn parse_value_at(bytes: &[u8], pos: &mut usize) -> Option<RawValue> {
    let start = skip_ws(bytes, *pos);
    if start >= bytes.len() {
        return None;
    }
    match bytes[start] {
        b'<' => {
            if bytes.get(start + 1) == Some(&b'<') {
                let (entries, end) = parse_dict_at(bytes, start)?;
                *pos = end;
                Some(RawValue::Dict(entries))
            } else {
                // Hex string: decode into bytes.
                let end = find_sub(bytes, b">", start + 1)?;
                let hex: String = String::from_utf8_lossy(&bytes[start + 1..end])
                    .chars()
                    .filter(|c| c.is_ascii_hexdigit())
                    .collect();
                let mut decoded = Vec::with_capacity(hex.len() / 2);
                let chars: Vec<char> = hex.chars().collect();
                let mut i = 0;
                while i + 1 < chars.len() {
                    let hi = chars[i].to_digit(16).unwrap_or(0) as u8;
                    let lo = chars[i + 1].to_digit(16).unwrap_or(0) as u8;
                    decoded.push((hi << 4) | lo);
                    i += 2;
                }
                if i < chars.len() {
                    let hi = chars[i].to_digit(16).unwrap_or(0) as u8;
                    decoded.push(hi << 4);
                }
                *pos = end + 1;
                Some(RawValue::Bytes(decoded))
            }
        }
        b'[' => {
            let mut i = start + 1;
            let mut items = Vec::new();
            loop {
                let j = skip_ws(bytes, i);
                if j >= bytes.len() {
                    i = j;
                    break;
                }
                if bytes[j] == b']' {
                    i = j + 1;
                    break;
                }
                let before = j;
                match parse_value_at(bytes, &mut i) {
                    Some(v) => items.push(v),
                    None => {
                        i = before + 1;
                    }
                }
            }
            *pos = i;
            Some(RawValue::Array(items))
        }
        b'(' => {
            let mut i = start + 1;
            let mut depth = 1usize;
            let mut decoded = Vec::new();
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    b'\\' => {
                        i += 2;
                        decoded.push(b'\\');
                    }
                    b'(' => {
                        depth += 1;
                        i += 1;
                        decoded.push(b'(');
                    }
                    b')' => {
                        depth -= 1;
                        i += 1;
                        decoded.push(b')');
                    }
                    other => {
                        i += 1;
                        decoded.push(other);
                    }
                }
            }
            *pos = i.min(bytes.len());
            Some(RawValue::Bytes(decoded))
        }
        b'/' => {
            let mut i = start + 1;
            while i < bytes.len()
                && !(bytes[i] as char).is_whitespace()
                && !b"/<>()[]{}%".contains(&bytes[i])
            {
                i += 1;
            }
            let name = String::from_utf8_lossy(&bytes[start + 1..i]).into_owned();
            *pos = i;
            Some(RawValue::Name(name))
        }
        b'+' | b'-' | b'.' | b'0'..=b'9' => {
            let mut i = start;
            if bytes[i] == b'+' || bytes[i] == b'-' {
                i += 1;
            }
            while i < bytes.len()
                && ((bytes[i] as char).is_ascii_digit()
                    || bytes[i] == b'.'
                    || bytes[i] == b'e'
                    || bytes[i] == b'E'
                    || ((bytes[i] == b'+' || bytes[i] == b'-')
                        && i > start
                        && matches!(bytes[i - 1], b'e' | b'E')))
            {
                i += 1;
            }
            let text = String::from_utf8_lossy(&bytes[start..i]);
            *pos = i;
            let Ok(number) = text.parse::<f64>() else {
                return Some(RawValue::Other);
            };
            // Look ahead for the `G R` of an indirect reference.
            let k = skip_ws(bytes, i);
            let mut m = k;
            while m < bytes.len() && bytes[m].is_ascii_digit() {
                m += 1;
            }
            if m > k {
                let r = skip_ws(bytes, m);
                if bytes.get(r) == Some(&b'R')
                    && !bytes.get(r + 1).is_some_and(|b| b.is_ascii_alphanumeric())
                {
                    *pos = r + 1;
                    return Some(RawValue::Reference(number as u32));
                }
            }
            Some(RawValue::Number(number))
        }
        _ => {
            // Keyword (`true`, `false`, `null`) or an unrecognised token.
            let mut i = start;
            while i < bytes.len()
                && !(bytes[i] as char).is_whitespace()
                && !b"/<>()[]{}".contains(&bytes[i])
            {
                i += 1;
            }
            *pos = i.max(start + 1);
            Some(RawValue::Other)
        }
    }
}

/// Skip ASCII whitespace.
fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i] as char).is_whitespace() {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_object_body_at_token_boundary() {
        let pdf = b"1 0 obj\n<< /A 1 >>\nendobj\n12 0 obj\n<< /B 2 >>\nendobj\n";
        let body = find_object_body(pdf, 12).unwrap();
        assert!(std::str::from_utf8(body).unwrap().contains("/B 2"));
        // Object 2 must not match inside "12 0 obj".
        assert!(find_object_body(pdf, 2).is_none());
    }

    #[test]
    fn finds_stream_span_without_trimming_payload() {
        // Payload ends with a whitespace byte that must be preserved.
        let pdf = b"5 0 obj\n<< /Length 3 >>\nstream\nab \nendstream\nendobj\n";
        let (s, e) = find_stream_span(pdf, 5).unwrap();
        assert_eq!(&pdf[s..e], b"ab ");
    }

    #[test]
    fn parses_nested_dictionaries() {
        let mut bytes =
            b"<< /Resources << /XObject << /Im1 7 0 R >> >> /Font << /F1 3 0 R >> >>".to_vec();
        bytes.push(0);
        let (entries, _) = parse_dict_at(&bytes, 0).unwrap();
        let value = RawValue::Dict(entries);
        let res = value.get("Resources").unwrap();
        let xobj = res.get("XObject").unwrap();
        assert_eq!(xobj.get("Im1").unwrap().as_reference(), Some(7));
        let font = value.get("Font").unwrap();
        assert_eq!(font.get("F1").unwrap().as_reference(), Some(3));
    }

    #[test]
    fn parses_numbers_names_and_arrays() {
        let mut bytes = b"<< /W 10 /H 20 /CS /DeviceRGB /DP [1 0 0 1 0 0] >>".to_vec();
        bytes.push(0);
        let (entries, _) = parse_dict_at(&bytes, 0).unwrap();
        let value = RawValue::Dict(entries);
        assert_eq!(value.get("W").unwrap().as_number(), Some(10.0));
        assert_eq!(
            value.get("CS").unwrap(),
            &RawValue::Name("DeviceRGB".into())
        );
        assert!(matches!(value.get("DP").unwrap(), RawValue::Array(a) if a.len() == 6));
    }
}
