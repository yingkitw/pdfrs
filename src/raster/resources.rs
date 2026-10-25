//! Page resource resolution: `/Resources` → `/ExtGState` and `/XObject`.
//!
//! Resources are resolved from the raw PDF bytes (see [`super::raw`]) because
//! the crate's general-purpose dictionary parser flattens nested dictionaries
//! and cannot represent `/Resources << /XObject << /Im1 7 0 R >> >>`.

use std::collections::HashMap;

use super::raw::{RawValue, find_object_body, parse_dict_at, parse_value_at};

/// The subset of an `/ExtGState` dictionary the rasterizer understands.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ExtGState {
    /// Constant alpha for fill operations (`/ca`).
    pub fill_alpha: Option<f32>,
    /// Constant alpha for stroke operations (`/CA`).
    pub stroke_alpha: Option<f32>,
    /// Line width override (`/LW`).
    pub line_width: Option<f32>,
}

/// Resources referenced by a page's `/Resources` dictionary.
#[derive(Debug, Clone, Default)]
pub(super) struct PageResources {
    /// `/ExtGState` entries, keyed by resource name (`/GS1` → `GS1`).
    pub ext_gstates: HashMap<String, ExtGState>,
    /// `/XObject` entries: resource name → object number of the XObject.
    pub xobjects: HashMap<String, u32>,
}

impl PageResources {
    /// Look up an `/ExtGState` by resource name.
    pub(super) fn ext_gstate(&self, name: &str) -> Option<&ExtGState> {
        self.ext_gstates.get(name)
    }
}

/// Collect the resources referenced by `page_id`.
pub(super) fn collect_page_resources(pdf_bytes: &[u8], page_id: u32) -> PageResources {
    let mut resources = PageResources::default();
    let Some(page_body) = find_object_body(pdf_bytes, page_id) else {
        return resources;
    };
    let Some(entries) = parse_body_dict(page_body) else {
        return resources;
    };
    let page = RawValue::Dict(entries);
    let Some(resources_value) = page.get("Resources") else {
        return resources;
    };
    let Some(resources_dict) = resolve_dict(pdf_bytes, resources_value) else {
        return resources;
    };

    if let Some(ext_gstate) = resources_dict.get("ExtGState")
        && let Some(RawValue::Dict(entries)) = resolve_dict(pdf_bytes, ext_gstate)
    {
        for (name, value) in entries {
            if let Some(RawValue::Dict(dict)) = resolve_dict(pdf_bytes, &value) {
                resources.ext_gstates.insert(name, read_ext_gstate(&dict));
            }
        }
    }

    if let Some(xobject) = resources_dict.get("XObject")
        && let Some(RawValue::Dict(entries)) = resolve_dict(pdf_bytes, xobject)
    {
        for (name, value) in entries {
            if let Some(id) = value.as_reference() {
                resources.xobjects.insert(name, id);
            }
        }
    }

    resources
}

/// Parse the dictionary at the start of a raw object body.
///
/// The body begins right after the `N G obj` keyword, so leading whitespace
/// is skipped before the `<<`.
fn parse_body_dict(body: &[u8]) -> Option<Vec<(String, RawValue)>> {
    let mut i = 0usize;
    while i < body.len() && (body[i] as char).is_whitespace() {
        i += 1;
    }
    let (entries, _) = parse_dict_at(body, i)?;
    Some(entries)
}

/// Resolve a value to a dictionary, following an indirect reference when
/// necessary.
fn resolve_dict(pdf_bytes: &[u8], value: &RawValue) -> Option<RawValue> {
    match value {
        RawValue::Dict(entries) => Some(RawValue::Dict(entries.clone())),
        RawValue::Reference(id) => {
            let body = find_object_body(pdf_bytes, *id)?;
            Some(RawValue::Dict(parse_body_dict(body)?))
        }
        _ => None,
    }
}

/// Read the alpha and line-width entries of an `/ExtGState` dictionary.
fn read_ext_gstate(dict: &[(String, RawValue)]) -> ExtGState {
    let mut state = ExtGState::default();
    for (key, value) in dict {
        let Some(number) = value.as_number() else {
            continue;
        };
        match key.as_str() {
            "ca" => state.fill_alpha = Some(number as f32),
            "CA" => state.stroke_alpha = Some(number as f32),
            "LW" => state.line_width = Some(number as f32),
            _ => {}
        }
    }
    state
}

/// Read the `/Filter` entry of a stream dictionary as a list of filter names.
pub(super) fn read_filters(dict: &[(String, RawValue)]) -> Vec<String> {
    let Some(value) = dict.iter().find(|(k, _)| k == "Filter").map(|(_, v)| v) else {
        return Vec::new();
    };
    match value {
        RawValue::Name(name) => vec![name.clone()],
        RawValue::Array(items) => items
            .iter()
            .filter_map(|v| match v {
                RawValue::Name(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Read the `/DecodeParms` predictor for a stream, if present.
///
/// Returns `(predictor, colors, bits_per_component, columns)`.
pub(super) fn read_decode_parms(
    dict: &[(String, RawValue)],
    pdf_bytes: &[u8],
) -> Option<(u32, usize, usize, usize)> {
    let value = dict
        .iter()
        .find(|(k, _)| k == "DecodeParms")
        .map(|(_, v)| v)?;
    let parms = match value {
        RawValue::Dict(entries) => entries.clone(),
        RawValue::Reference(id) => {
            let body = find_object_body(pdf_bytes, *id)?;
            let (entries, _) = parse_dict_at(body, 0)?;
            entries
        }
        RawValue::Array(items) => match items.first() {
            Some(RawValue::Dict(entries)) => entries.clone(),
            Some(RawValue::Reference(id)) => {
                let body = find_object_body(pdf_bytes, *id)?;
                parse_body_dict(body)?
            }
            _ => return None,
        },
        _ => return None,
    };
    let get = |key: &str| {
        parms
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.as_number())
    };
    let predictor = get("Predictor").unwrap_or(1.0) as u32;
    let colors = get("Colors").unwrap_or(1.0) as usize;
    let bpc = get("BitsPerComponent").unwrap_or(8.0) as usize;
    let columns = get("Columns").unwrap_or(1.0) as usize;
    Some((predictor, colors, bpc, columns))
}

/// Parse the dictionary that precedes a stream keyword in an object body.
///
/// The body of a stream object starts with its dictionary, so the dictionary is
/// the leading `<< ... >>` region.
pub(super) fn read_stream_dict(body: &[u8]) -> Vec<(String, RawValue)> {
    let mut i = 0usize;
    while i < body.len() && (body[i] as char).is_whitespace() {
        i += 1;
    }
    if body.get(i) != Some(&b'<') || body.get(i + 1) != Some(&b'<') {
        return Vec::new();
    }
    match parse_dict_at(body, i) {
        Some((entries, _)) => entries,
        None => Vec::new(),
    }
}

/// Read a single dictionary entry as a number.
pub(super) fn dict_number(dict: &[(String, RawValue)], key: &str) -> Option<f64> {
    dict.iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_number())
}

/// Read a single dictionary entry as a name.
pub(super) fn dict_name(dict: &[(String, RawValue)], key: &str) -> Option<String> {
    dict.iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| match v {
            RawValue::Name(name) => Some(name.clone()),
            _ => None,
        })
}

/// Parse the value that follows `key` in a raw object body.
pub(super) fn value_after_key(body: &[u8], key: &str) -> Option<RawValue> {
    let needle = format!("/{key}");
    let bytes = needle.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = super::raw::find_sub(body, bytes, from) {
        let pos = rel + bytes.len();
        // Ensure the match is a whole key (not a prefix of a longer name).
        if body
            .get(pos)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
        {
            from = rel + 1;
            continue;
        }
        let mut cursor = pos;
        if let Some(value) = parse_value_at(body, &mut cursor) {
            return Some(value);
        }
        from = rel + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_ext_gstate_and_xobject_from_inline_resources() {
        let pdf = b"4 0 obj\n<< /Type /Page /Resources << /ExtGState << /GS1 << /ca 0.5 /CA 0.25 >> >> /XObject << /Im1 9 0 R >> >> >>\nendobj\n";
        let resources = collect_page_resources(pdf, 4);
        let gs = resources.ext_gstate("GS1").expect("GS1");
        assert_eq!(gs.fill_alpha, Some(0.5));
        assert_eq!(gs.stroke_alpha, Some(0.25));
        assert_eq!(resources.xobjects.get("Im1"), Some(&9));
    }

    #[test]
    fn collects_resources_from_indirect_resource_object() {
        let pdf = b"4 0 obj\n<< /Type /Page /Resources 8 0 R >>\nendobj\n\
                    8 0 obj\n<< /XObject << /Im2 11 0 R >> /ExtGState << /GS0 << /ca 0.75 >> >> >>\nendobj\n";
        let resources = collect_page_resources(pdf, 4);
        assert_eq!(resources.xobjects.get("Im2"), Some(&11));
        assert_eq!(
            resources.ext_gstate("GS0").and_then(|g| g.fill_alpha),
            Some(0.75)
        );
    }

    #[test]
    fn reads_filter_and_decode_parms() {
        let dict = vec![
            ("Filter".to_string(), RawValue::Name("FlateDecode".into())),
            (
                "DecodeParms".to_string(),
                RawValue::Dict(vec![
                    ("Predictor".to_string(), RawValue::Number(15.0)),
                    ("Colors".to_string(), RawValue::Number(3.0)),
                    ("Columns".to_string(), RawValue::Number(4.0)),
                ]),
            ),
        ];
        assert_eq!(read_filters(&dict), vec!["FlateDecode".to_string()]);
        let (predictor, colors, _bpc, columns) = read_decode_parms(&dict, b"").unwrap();
        assert_eq!((predictor, colors, columns), (15, 3, 4));
    }

    #[test]
    fn missing_page_yields_empty_resources() {
        let resources = collect_page_resources(b"nothing here", 3);
        assert!(resources.xobjects.is_empty());
        assert!(resources.ext_gstates.is_empty());
    }
}
