//! PDF object access helpers: MediaBox geometry and dict/array/number access.

use crate::error::{PdfError, Result};
use crate::pdf::{PdfDocument, PdfObject, PdfValue};
use std::collections::HashMap;

/// Returns `(width_pt, height_pt)` of a page's MediaBox.
pub(super) fn page_media_box(
    pdf_bytes: &[u8],
    doc: &PdfDocument,
    page_id: u32,
) -> Result<(f32, f32)> {
    // Scan the raw PDF for the first /MediaBox inside the page object body.
    // The bundled dict parser truncates bracket arrays at the first whitespace,
    // so we go straight to the source text.
    if let Some(values) = raw_mediabox(pdf_bytes, page_id)
        && values.len() >= 4
    {
        return Ok((values[2] - values[0], values[3] - values[1]));
    }

    // Fallback to the parsed dict (in case the PDF was already structured).
    let dict = object_dict(doc, page_id)
        .ok_or_else(|| PdfError::Parse(format!("page {page_id} not a dictionary")))?;
    if let Some(mb) = dict.get("MediaBox") {
        let values: Vec<f32> = if let Some(arr) = as_array(doc, mb) {
            arr.iter().filter_map(|v| as_number(doc, v)).collect()
        } else if let PdfValue::Object(PdfObject::String(s)) = mb {
            parse_bracket_or_numbers(s)
        } else if let PdfValue::Reference(id, _) = mb
            && let Some(obj) = doc.objects.get(id)
        {
            match obj {
                PdfObject::Array(items) => items.iter().filter_map(|v| as_number(doc, v)).collect(),
                PdfObject::String(s) => parse_bracket_or_numbers(s),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        if values.len() >= 4 {
            return Ok((values[2] - values[0], values[3] - values[1]));
        }
    }
    Ok((612.0, 792.0))
}

/// Search the raw PDF text for `/MediaBox [..]` inside the given page object
/// body and parse the four numbers.
fn raw_mediabox(pdf_bytes: &[u8], page_id: u32) -> Option<Vec<f32>> {
    let needle = format!("{} 0 obj", page_id);
    let text = String::from_utf8_lossy(pdf_bytes);
    let obj_start = text.find(&needle)?;
    let after = &text[obj_start + needle.len()..];
    let obj_end_rel = after.find("endobj")?;
    let body = &after[..obj_end_rel];

    let mb_rel = body.find("/MediaBox")?;
    let after_mb = &body[mb_rel + "/MediaBox".len()..];
    let bracket_open = after_mb.find('[')?;
    let after_open = &after_mb[bracket_open + 1..];
    let bracket_close = after_open.find(']')?;
    let inside = &after_open[..bracket_close];
    Some(parse_bracket_or_numbers(inside))
}

/// Parse either `[a b c d]` bracket strings or `a b c d` whitespace-separated
/// number strings into a flat `Vec<f32>`.
fn parse_bracket_or_numbers(s: &str) -> Vec<f32> {
    let trimmed = s.trim();
    let inner = trimmed.trim_start_matches('[').trim_end_matches(']').trim();
    inner
        .split_whitespace()
        .filter_map(|tok| tok.parse::<f32>().ok())
        .collect()
}

pub(super) fn object_dict(doc: &PdfDocument, id: u32) -> Option<&HashMap<String, PdfValue>> {
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

pub(super) fn as_number(doc: &PdfDocument, val: &PdfValue) -> Option<f32> {
    match val {
        PdfValue::Object(PdfObject::Number(n)) => Some(*n as f32),
        PdfValue::Reference(id, _) => doc.objects.get(id).and_then(|o| {
            if let PdfObject::Number(n) = o {
                Some(*n as f32)
            } else {
                None
            }
        }),
        _ => None,
    }
}

pub(super) fn as_ref_id(val: &PdfValue) -> Option<u32> {
    match val {
        PdfValue::Reference(id, _) => Some(*id),
        PdfValue::Object(PdfObject::Reference(id, _)) => Some(*id),
        PdfValue::Object(PdfObject::String(s)) => parse_ref_str(s),
        _ => None,
    }
}

/// Parse an `N G R` reference literal, a bare numeric id, or a `[N]` bracket
/// literal out of a string. Returns the first parseable object id.
pub(super) fn parse_ref_str(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.starts_with('[') {
        // Find first numeric inside brackets
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
