//! Low-level PDF object scanning used when encrypting an existing document.
//!
//! Encryption has to rewrite every string and stream in the file, which
//! means locating the objects, expanding compressed object/xref streams and
//! rebuilding the cross-reference table. These helpers work directly on the
//! raw bytes so binary payloads survive untouched.

use super::regexes::{re_info_ref, re_root_ref};
use crate::error::{PdfError, Result};

/// A scanned `N G obj ... endobj` object with byte-precise extents.
pub(super) struct RawObject {
    pub(super) num: u32,
    pub(super) generation: u16,
    pub(super) header_start: usize,
    /// Just after the `obj` keyword.
    pub(super) header_end: usize,
    /// Just after `endobj`.
    pub(super) body_end: usize,
    pub(super) stream: Option<StreamSpan>,
    pub(super) body_override: Option<Vec<u8>>,
    pub(super) skip: bool,
    pub(super) is_xref_stream: bool,
}

pub(super) struct StreamSpan {
    /// Position of the `stream` keyword.
    pub(super) keyword_start: usize,
    pub(super) data_start: usize,
    pub(super) data_end: usize,
}

impl RawObject {
    pub(super) fn dict_slice<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        match self.stream {
            Some(ref s) => &bytes[self.header_end..s.keyword_start],
            None => &bytes[self.header_end..self.body_end],
        }
    }
}

pub(super) fn find_from(bytes: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Scan `N G obj ... endobj` objects sequentially, using direct `/Length`
/// values to locate stream boundaries in binary data.
pub(super) fn scan_pdf_objects(bytes: &[u8]) -> Result<Vec<RawObject>> {
    let header_re = regex::bytes::Regex::new(r"(\d+)[\x20\t\r\n]+(\d+)[\x20\t\r\n]+obj").unwrap();
    let mut objects = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let Some(m) = header_re.find_at(bytes, pos) else {
            break;
        };
        let whole = &bytes[m.start()..m.end()];
        let fields: Vec<&[u8]> = whole
            .split(|&b| b == b' ' || b == b'\t' || b == b'\r' || b == b'\n')
            .filter(|f| !f.is_empty())
            .collect();
        if fields.len() < 3 {
            pos = m.end();
            continue;
        }
        let num: u32 = std::str::from_utf8(fields[0])
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        let generation: u16 = std::str::from_utf8(fields[1])
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        if num == 0 {
            pos = m.end();
            continue;
        }

        let endobj = find_from(bytes, b"endobj", m.end());
        let stream_kw = find_standalone_stream_kw(bytes, m.end());

        let (stream, body_end) = match (endobj, stream_kw) {
            (Some(eo), Some(sk)) if sk < eo => {
                let dict = &bytes[m.end()..sk];
                let mut data_start = sk + 6;
                if bytes.get(data_start) == Some(&b'\r') {
                    data_start += 1;
                }
                if bytes.get(data_start) == Some(&b'\n') {
                    data_start += 1;
                }
                let data_end = match stream_data_end_via_length(bytes, dict, data_start) {
                    Some(end) => end,
                    None => {
                        let es = find_from(bytes, b"endstream", data_start).ok_or_else(|| {
                            PdfError::Parse(format!("unterminated stream in object {num}"))
                        })?;
                        strip_trailing_eol(bytes, es)
                    }
                };
                let after_data = find_from(bytes, b"endstream", data_end)
                    .ok_or_else(|| PdfError::Parse(format!("missing endstream in object {num}")))?;
                let end = find_from(bytes, b"endobj", after_data)
                    .ok_or_else(|| PdfError::Parse(format!("missing endobj for object {num}")))?
                    + 6;
                (
                    Some(StreamSpan {
                        keyword_start: sk,
                        data_start,
                        data_end,
                    }),
                    end,
                )
            }
            (Some(eo), _) => (None, eo + 6),
            (None, _) => {
                return Err(PdfError::Parse(format!(
                    "unterminated object near offset {}",
                    m.start()
                )));
            }
        };

        objects.push(RawObject {
            num,
            generation,
            header_start: m.start(),
            header_end: m.end(),
            body_end,
            stream,
            body_override: None,
            skip: false,
            is_xref_stream: false,
        });
        pos = body_end;
    }
    Ok(objects)
}

/// Find the next `stream` keyword that stands alone (whitespace before,
/// end-of-line after) — avoids matching the word inside stream data.
pub(super) fn find_standalone_stream_kw(bytes: &[u8], from: usize) -> Option<usize> {
    let mut search = from;
    while let Some(p) = find_from(bytes, b"stream", search) {
        let before_ok = p > 0 && bytes[p - 1].is_ascii_whitespace();
        let after = p + 6;
        let after_ok = matches!(bytes.get(after), Some(b'\r') | Some(b'\n'));
        if before_ok && after_ok {
            return Some(p);
        }
        search = p + 6;
    }
    None
}

/// Resolve the stream data end using a direct `/Length` integer, verifying
/// that `endstream` actually follows. Returns `None` to fall back to a scan.
pub(super) fn stream_data_end_via_length(
    bytes: &[u8],
    dict: &[u8],
    data_start: usize,
) -> Option<usize> {
    let re = regex::bytes::Regex::new(r"/Length\s+(\d+)").ok()?;
    let caps = re.captures(dict)?;
    let len: usize = std::str::from_utf8(&caps[1]).ok()?.parse().ok()?;
    let end = data_start.checked_add(len)?;
    let after = &bytes[end.min(bytes.len())..];
    let ws = after
        .iter()
        .take_while(|&&b| b == b'\r' || b == b'\n')
        .count();
    if after[ws..].starts_with(b"endstream") {
        Some(end)
    } else {
        None
    }
}

pub(super) fn strip_trailing_eol(bytes: &[u8], endstream_pos: usize) -> usize {
    let mut end = endstream_pos;
    if end > 0 && bytes[end - 1] == b'\n' {
        end -= 1;
    }
    if end > 0 && bytes[end - 1] == b'\r' {
        end -= 1;
    }
    end
}

pub(super) fn is_object_stream_dict(dict: &[u8]) -> bool {
    dict.windows(b"/Type /ObjStm".len())
        .any(|window| window == b"/Type /ObjStm")
        || dict
            .windows(b"/Type/ObjStm".len())
            .any(|window| window == b"/Type/ObjStm")
}

pub(super) fn is_xref_stream_dict(dict: &[u8]) -> bool {
    dict.windows(b"/Type /XRef".len())
        .any(|window| window == b"/Type /XRef")
        || dict
            .windows(b"/Type/XRef".len())
            .any(|window| window == b"/Type/XRef")
}

/// Expand object streams into regular objects and discard xref streams.
///
/// The encrypted output is rebuilt with a classic xref table, so compressed
/// objects must be materialized before encryption. Xref stream data is metadata
/// replaced by the newly generated classic table.
pub(super) fn expand_compressed_objects(objects: &mut Vec<RawObject>, bytes: &[u8]) -> Result<()> {
    let mut expanded = Vec::new();
    for obj in objects.iter_mut() {
        let Some(stream) = obj.stream.as_ref() else {
            continue;
        };
        let dict = obj.dict_slice(bytes);
        let is_obj_stream = is_object_stream_dict(dict);
        let is_xref_stream = is_xref_stream_dict(dict);
        if is_xref_stream {
            obj.is_xref_stream = true;
            continue;
        }
        if !is_obj_stream {
            continue;
        }

        let number = |name: &[u8]| -> Option<usize> {
            let pattern = format!(r"{}\s+(\d+)", String::from_utf8_lossy(name));
            let re = regex::bytes::Regex::new(&pattern).ok()?;
            let captures = re.captures(dict)?;
            std::str::from_utf8(&captures[1]).ok()?.parse().ok()
        };
        let n = number(b"/N")
            .ok_or_else(|| PdfError::Parse(format!("object stream {} has no /N", obj.num)))?;
        let first = number(b"/First")
            .ok_or_else(|| PdfError::Parse(format!("object stream {} has no /First", obj.num)))?;
        let raw = &bytes[stream.data_start..stream.data_end];
        let data = crate::search::decompress_stream(raw);
        if first > data.len() {
            return Err(PdfError::Parse(format!(
                "object stream {} has invalid /First",
                obj.num
            )));
        }
        let header = &data[..first];
        let tokens: Vec<&[u8]> = header
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|token| !token.is_empty())
            .collect();
        if tokens.len() < n.saturating_mul(2) {
            return Err(PdfError::Parse(format!(
                "object stream {} has an incomplete header",
                obj.num
            )));
        }
        let mut entries = Vec::with_capacity(n);
        for index in 0..n {
            let number = std::str::from_utf8(tokens[index * 2])
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .ok_or_else(|| PdfError::Parse("invalid object stream object number".into()))?;
            let offset = std::str::from_utf8(tokens[index * 2 + 1])
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .ok_or_else(|| PdfError::Parse("invalid object stream object offset".into()))?;
            entries.push((number, offset));
        }
        for (index, (number, offset)) in entries.iter().enumerate() {
            let start = first.checked_add(*offset).ok_or_else(|| {
                PdfError::Parse(format!("object stream {} offset overflow", obj.num))
            })?;
            let end = entries
                .get(index + 1)
                .and_then(|(_, next)| first.checked_add(*next))
                .unwrap_or(data.len());
            if start > end || end > data.len() {
                return Err(PdfError::Parse(format!(
                    "object stream {} has invalid object bounds",
                    obj.num
                )));
            }
            let body = trim_pdf_whitespace(&data[start..end]).to_vec();
            expanded.push(RawObject {
                num: *number,
                generation: 0,
                header_start: 0,
                header_end: 0,
                body_end: 0,
                stream: None,
                body_override: Some(body),
                skip: false,
                is_xref_stream: false,
            });
        }
        obj.skip = true;
    }
    objects.extend(expanded);
    Ok(())
}

pub(super) fn trim_pdf_whitespace(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

/// Extract `/Root` and `/Info` references from a trailer or xref-stream dict.
pub(super) fn parse_trailer_refs(
    bytes: &[u8],
    objects: &[RawObject],
) -> Result<(String, Option<String>)> {
    let sx = find_from(bytes, b"startxref", 0)
        .ok_or_else(|| PdfError::InvalidPdf("no startxref found in document".into()))?;
    let dict = if let Some(trailer_kw) = find_from(bytes, b"trailer", 0).filter(|&t| t < sx) {
        let dict_re = regex::bytes::Regex::new(r"(?s)<<(.+?)>>").unwrap();
        let region = &bytes[trailer_kw..sx];
        dict_re
            .captures(region)
            .map(|c| c[1].to_vec())
            .ok_or_else(|| PdfError::Parse("malformed trailer dictionary".into()))?
    } else {
        objects
            .iter()
            .find(|obj| obj.is_xref_stream)
            .map(|obj| obj.dict_slice(bytes).to_vec())
            .ok_or_else(|| {
                PdfError::Unsupported(
                    "no trailer dictionary or cross-reference stream found".into(),
                )
            })?
    };
    let dict_text = String::from_utf8_lossy(&dict).to_string();
    let root_re = re_root_ref();
    let root = root_re
        .captures(&dict_text)
        .map(|c| c[1].to_string())
        .ok_or_else(|| PdfError::Parse("trailer has no /Root reference".into()))?;
    let info_re = re_info_ref();
    let info = info_re.captures(&dict_text).map(|c| c[1].to_string());
    Ok((root, info))
}

/// Replace a stream dictionary `/Length` entry with a new direct value.
pub(super) fn rewrite_stream_length(dict: &[u8], new_len: usize) -> Vec<u8> {
    let re = regex::bytes::Regex::new(r"(/Length\s+)\d+(?:\s+\d+\s+R)?").unwrap();
    re.replace_all(dict, format!("${{1}}{new_len}").as_bytes())
        .into_owned()
}

/// Encrypt PDF literal string `(...)` values within an object dictionary,
/// re-emitting them as hex strings. Handles escape sequences and nesting.
pub(super) fn encrypt_strings_in_dict(
    dict: &[u8],
    security: &crate::security::PdfSecurity,
    file_key: &[u8],
    obj_num: u32,
    gen_num: u16,
) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(dict.len());
    let mut i = 0usize;
    while i < dict.len() {
        if dict[i] != b'(' {
            out.push(dict[i]);
            i += 1;
            continue;
        }
        // Scan the literal string, decoding escapes into raw bytes.
        let mut depth = 1usize;
        let mut raw = Vec::new();
        i += 1;
        while i < dict.len() && depth > 0 {
            let c = dict[i];
            if c == b'\\' && i + 1 < dict.len() {
                let n = dict[i + 1];
                match n {
                    b'n' => raw.push(b'\n'),
                    b'r' => raw.push(b'\r'),
                    b't' => raw.push(b'\t'),
                    b'b' => raw.push(0x08),
                    b'f' => raw.push(0x0C),
                    b'(' => raw.push(b'('),
                    b')' => raw.push(b')'),
                    b'\\' => raw.push(b'\\'),
                    b'0'..=b'7' => {
                        let mut val = 0u32;
                        let mut used = 0;
                        let mut j = i + 1;
                        while used < 3 && j < dict.len() && (b'0'..=b'7').contains(&dict[j]) {
                            val = val * 8 + u32::from(dict[j] - b'0');
                            j += 1;
                            used += 1;
                        }
                        raw.push((val & 0xFF) as u8);
                        i = j - 2;
                    }
                    b'\r' => {
                        if dict.get(i + 2) == Some(&b'\n') {
                            i += 1;
                        }
                    }
                    b'\n' => {}
                    other => raw.push(other),
                }
                i += 2;
            } else if c == b'(' {
                depth += 1;
                raw.push(c);
                i += 1;
            } else if c == b')' {
                depth -= 1;
                if depth > 0 {
                    raw.push(c);
                }
                i += 1;
            } else {
                raw.push(c);
                i += 1;
            }
        }
        if depth != 0 {
            return Err(PdfError::Parse(format!(
                "unbalanced string literal in object {obj_num}"
            )));
        }
        let encrypted = security.encrypt_data(&raw, file_key, obj_num, gen_num)?;
        out.push(b'<');
        for b in &encrypted {
            out.extend_from_slice(format!("{b:02x}").as_bytes());
        }
        out.push(b'>');
    }
    Ok(out)
}
