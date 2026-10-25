//! Page discovery and shared PDF-object helpers (kids, refs, stream decompression).
use crate::error::{PdfError, Result};
use crate::pdf::{PdfDocument, PdfObject, PdfValue};
use std::collections::HashMap;

/// A rectangle on a PDF page (in PDF user-space points, origin bottom-left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn right(&self) -> f32 {
        self.x + self.width
    }
    pub fn top(&self) -> f32 {
        self.y + self.height
    }
    /// True if this rectangle intersects `other` (touching counts as intersection).
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x <= other.right()
            && other.x <= self.right()
            && self.y <= other.top()
            && other.y <= self.top()
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.right() && py >= self.y && py <= self.top()
    }
}

/// A single search match.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Zero-indexed page number in document order.
    pub page: usize,
    /// Matched substring as it appears in the content stream (decoded).
    pub text: String,
    /// Surrounding context (up to ~24 chars on each side, with ellipses when truncated).
    pub snippet: String,
    /// Bounding box of the matched characters in PDF user-space points.
    pub bbox: Rect,
}
pub fn collect_pages(doc: &PdfDocument) -> Vec<u32> {
    collect_pages_from_doc(doc, None)
}

/// Collect pages with an optional raw PDF buffer to recover from the
/// whitespace-tokenised dict parser (which truncates `/Kids [a b c]` to `[a`).
pub(crate) fn collect_pages_from_doc(doc: &PdfDocument, raw: Option<&[u8]>) -> Vec<u32> {
    let mut pages = Vec::new();
    let mut stack: Vec<u32> = Vec::new();
    if let Some(catalog) = object_dict(doc, doc.catalog)
        && let Some(pages_ref) = catalog.get("Pages")
        && let Some(pages_id) = as_ref_id(pages_ref)
    {
        stack.push(pages_id);
    }
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(dict) = object_dict(doc, id) else {
            continue;
        };
        let type_val = dict.get("Type").and_then(|v| match v {
            PdfValue::Object(PdfObject::Name(n)) => Some(n.clone()),
            PdfValue::Object(PdfObject::String(s)) => {
                Some(s.trim().trim_start_matches('/').to_string())
            }
            _ => None,
        });
        if let Some(t) = type_val {
            if t == "Page" {
                pages.push(id);
                continue;
            }
            if t != "Pages" {
                continue;
            }
        }
        // Try the parsed dict first.
        let mut resolved: Vec<u32> = Vec::new();
        if let Some(kids) = dict.get("Kids") {
            if let Some(arr) = as_array(doc, kids) {
                resolved.extend(arr.iter().filter_map(as_ref_id));
            } else if let PdfValue::Object(PdfObject::String(s)) = kids {
                resolved.extend(parse_kids_string(s));
            }
        }
        // If the dict parser truncated the Kids array, scan the raw PDF buffer.
        if resolved.len() < 2
            && let Some(raw) = raw
            && let Some(raw_kids) = raw_kids_for_object(raw, id)
            && raw_kids.len() > resolved.len()
        {
            resolved = raw_kids;
        }
        for kid_id in resolved {
            stack.push(kid_id);
        }
    }
    pages.sort();
    pages
}

/// Scan the raw PDF for `N 0 obj ... /Kids [..] ... endobj` and return the
/// full list of object IDs inside the brackets. Used to recover array values
/// that the whitespace-only dict parser truncated.
pub(crate) fn raw_kids_for_object(pdf_bytes: &[u8], obj_id: u32) -> Option<Vec<u32>> {
    let text = String::from_utf8_lossy(pdf_bytes);
    let needle = format!("{} 0 obj", obj_id);
    let obj_start = text.find(&needle)?;
    let after = &text[obj_start + needle.len()..];
    let obj_end_rel = after.find("endobj")?;
    let body = &after[..obj_end_rel];
    let kids_rel = body.find("/Kids")?;
    let after_kids = &body[kids_rel + "/Kids".len()..];
    let open = after_kids.find('[')?;
    let after_open = &after_kids[open + 1..];
    let close = after_open.find(']')?;
    let inside = &after_open[..close];
    Some(parse_kids_string(inside))
}

pub(crate) fn page_content_streams(doc: &PdfDocument, page_id: u32) -> Result<Vec<u32>> {
    let dict = object_dict(doc, page_id)
        .ok_or_else(|| PdfError::Parse(format!("page {page_id} not a dictionary")))?;
    let mut out = Vec::new();
    if let Some(contents) = dict.get("Contents") {
        match contents {
            PdfValue::Reference(id, _) => out.push(*id),
            PdfValue::Object(PdfObject::Array(items)) => {
                for item in items {
                    if let Some(id) = as_ref_id(item) {
                        out.push(id);
                    }
                }
            }
            PdfValue::Object(PdfObject::String(s)) => {
                // Whitespace-token dict may store contents as a string
                // like "6 0 R" or "[6 7 0 R]" or just "6".
                if s.trim().starts_with('[') {
                    out.extend(parse_kids_string(s));
                } else if let Some(id) = as_ref_id(contents) {
                    out.push(id);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

pub(crate) fn object_dict(doc: &PdfDocument, id: u32) -> Option<&HashMap<String, PdfValue>> {
    doc.objects.get(&id).and_then(|o| match o {
        PdfObject::Dictionary(d) => Some(d),
        PdfObject::Stream { dictionary, .. } => Some(dictionary),
        _ => None,
    })
}

pub(super) fn as_array(doc: &PdfDocument, val: &PdfValue) -> Option<Vec<PdfValue>> {
    match val {
        PdfValue::Object(PdfObject::Array(items)) => Some(items.clone()),
        PdfValue::Reference(id, _) => doc.objects.get(id).and_then(|o| {
            if let PdfObject::Array(items) = o {
                Some(items.clone())
            } else {
                None
            }
        }),
        _ => None,
    }
}

pub(crate) fn as_ref_id(val: &PdfValue) -> Option<u32> {
    match val {
        PdfValue::Reference(id, _) => Some(*id),
        PdfValue::Object(PdfObject::Reference(id, _)) => Some(*id),
        PdfValue::Object(PdfObject::String(s)) => parse_ref_str(s),
        _ => None,
    }
}

pub(crate) fn parse_ref_str(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.starts_with('[') {
        for tok in s.trim_matches(|c| c == '[' || c == ']').split_whitespace() {
            if let Ok(id) = tok.trim_end_matches('R').trim().parse::<u32>() {
                return Some(id);
            }
        }
        return None;
    }
    if let Some(first) = s.split_whitespace().next() {
        let candidate = first.trim_end_matches('R');
        if let Ok(id) = candidate.parse::<u32>() {
            return Some(id);
        }
    }
    None
}

pub(crate) fn parse_kids_string(s: &str) -> Vec<u32> {
    let inner = s.trim().trim_matches(|c| c == '[' || c == ']');
    inner
        .split_whitespace()
        .filter_map(|tok| tok.trim_end_matches('R').parse::<u32>().ok())
        .collect()
}

pub(crate) fn decompress_stream(data: &[u8]) -> Vec<u8> {
    if data.len() > 2
        && data[0] == 0x78
        && ((u16::from(data[0]) * 256) + u16::from(data[1])).is_multiple_of(31)
    {
        crate::compression::decompress_deflate(data).unwrap_or_else(|_| data.to_vec())
    } else {
        data.to_vec()
    }
}

/// Best-effort detection of zlib/FlateDecode-compressed stream bytes.
pub(crate) fn is_deflate_stream(data: &[u8]) -> bool {
    data.len() > 2 && data[0] == 0x78 && crate::compression::decompress_deflate(data).is_ok()
}
