//! Font metrics: width collection and embedded TrueType extraction.

use crate::pdf::{PdfDocument, PdfObject, PdfValue};
use crate::search::decompress_stream;
use std::collections::HashMap;
use std::sync::OnceLock;

use super::base14::{base14_font_metrics, is_base14_font};
use super::pdf_access::{as_array, as_number, as_ref_id, object_dict, parse_ref_str};

/// Locate substitute TrueType font bytes for rendering base-14 letterforms.
///
/// Base-14 fonts carry no embedded font program, so glyph *shapes* are
/// unavailable from the PDF itself. When a system font (or the font named by
/// `PDFRS_UNICODE_FONT_PATH`) can be read, its outlines stand in for base-14
/// glyphs so text renders as real letterforms instead of gray blocks. Widths
/// still come from the built-in base-14 tables, keeping layout identical.
pub(super) fn substitute_font_bytes() -> Option<&'static Vec<u8>> {
    static FONT: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    FONT.get_or_init(|| {
        let mut candidates = Vec::new();
        if let Ok(path) = std::env::var("PDFRS_UNICODE_FONT_PATH") {
            candidates.push(path);
        }
        candidates.extend(system_font_candidates());
        for path in candidates {
            if let Ok(bytes) = std::fs::read(&path)
                && ttf_parser::Face::parse(&bytes, 0).is_ok()
            {
                return Some(bytes);
            }
        }
        None
    })
    .as_ref()
}

fn system_font_candidates() -> Vec<String> {
    #[cfg(target_os = "macos")]
    let paths = [
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/SFNS.ttf",
        "/Library/Fonts/Arial.ttf",
    ];
    #[cfg(target_os = "linux")]
    let paths = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
    ];
    #[cfg(target_os = "windows")]
    let paths = [
        "C:\\Windows\\Fonts\\arial.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
    ];
    paths.iter().map(|s| s.to_string()).collect()
}

/// Attach substitute-font bytes to base-14 metrics (no-op when the font has
/// its own embedded program or no substitute is available).
pub(super) fn attach_substitute_font(metrics: &mut FontMetrics, font_name: &str) {
    if metrics.embedded_ttf.is_none() && is_base14_font(font_name) {
        metrics.embedded_ttf = substitute_font_bytes().cloned();
    }
}

#[derive(Debug, Clone)]
pub(super) struct FontMetrics {
    /// Width of every char in 1/1000 em (font units).
    pub(super) widths: HashMap<u32, u16>,
    /// Default width when a char isn't in the table.
    pub(super) default_width: u16,
    /// Raw embedded TrueType font bytes (from /FontFile2), if present.
    pub(super) embedded_ttf: Option<Vec<u8>>,
}

impl FontMetrics {
    pub(super) fn advance(&self, ch: u32) -> u16 {
        self.widths.get(&ch).copied().unwrap_or(self.default_width)
    }
}

fn collect_font_metrics(doc: &PdfDocument) -> HashMap<String, FontMetrics> {
    let mut out = HashMap::new();
    // Walk every object looking for Resources dictionaries (on pages, catalog, etc.)
    for (&_id, obj) in &doc.objects {
        let dict = match obj {
            PdfObject::Dictionary(d) => d,
            PdfObject::Stream { dictionary, .. } => dictionary,
            _ => continue,
        };
        if let Some(resources) = dict.get("Resources") {
            walk_resources(doc, resources, &mut out);
        }
    }
    out
}

/// Collect font metrics by scanning raw PDF bytes, working around the
/// whitespace-tokenised dict parser which truncates inline dictionaries.
/// Scans for /Font << /F1 ... >> on the page, then resolves each font
/// object's /FontFile2 to extract embedded TTF data.
pub(super) fn collect_font_metrics_with_raw(
    doc: &PdfDocument,
    pdf_bytes: &[u8],
    page_id: u32,
) -> HashMap<String, FontMetrics> {
    // First try the parsed-dict approach (works for well-structured PDFs).
    let mut out = collect_font_metrics(doc);
    if !out.is_empty() {
        return out;
    }

    // Fallback: scan raw PDF bytes for font definitions on this page.
    let text = String::from_utf8_lossy(pdf_bytes);
    let page_needle = format!("{} 0 obj", page_id);
    let Some(page_start) = text.find(&page_needle) else {
        return out;
    };
    let after_page = &text[page_start + page_needle.len()..];
    let Some(page_end) = after_page.find("endobj") else {
        return out;
    };
    let page_body = &after_page[..page_end];

    // Find /Font << ... >> or /Font N 0 R in the page body.
    if let Some(font_pos) = page_body.find("/Font") {
        let after_font = &page_body[font_pos + "/Font".len()..];

        // Case 1: /Font << /F1 N 0 R /F2 M 0 R ... >>
        if after_font.trim_start().starts_with("<<") {
            let dict_start = after_font.find("<<").unwrap();
            let after_dict = &after_font[dict_start + 2..];
            if let Some(dict_end) = find_matching_angle_brackets(after_dict) {
                let font_dict_body = &after_dict[..dict_end];
                // Parse /Name N 0 R pairs from the font dict body.
                let mut pos = 0;
                let chars: Vec<char> = font_dict_body.chars().collect();
                while pos < chars.len() {
                    if chars[pos] == '/' {
                        let name_start = pos + 1;
                        let mut name_end = name_start;
                        while name_end < chars.len()
                            && !chars[name_end].is_whitespace()
                            && chars[name_end] != '/'
                        {
                            name_end += 1;
                        }
                        let name: String = chars[name_start..name_end].iter().collect();
                        // Skip whitespace, then look for "N 0 R" or "N G R"
                        let mut p = name_end;
                        while p < chars.len() && chars[p].is_whitespace() {
                            p += 1;
                        }
                        // Parse reference number
                        let ref_start = p;
                        while p < chars.len() && chars[p].is_ascii_digit() {
                            p += 1;
                        }
                        if p > ref_start {
                            let num_str: String = chars[ref_start..p].iter().collect();
                            if let Ok(font_obj_id) = num_str.parse::<u32>() {
                                let metrics = font_metrics_from_raw(doc, pdf_bytes, font_obj_id);
                                out.insert(name, metrics);
                            }
                        }
                        pos = p;
                    } else {
                        pos += 1;
                    }
                }
            }
        }
        // Case 2: /Font N 0 R — font dict is an indirect object
        else {
            let trimmed = after_font.trim_start();
            if let Some(first_num) = trimmed.split_whitespace().next()
                && let Ok(font_dict_id) = first_num.parse::<u32>()
            {
                // The font dict is an indirect object — scan for it.
                let fd_needle = format!("{} 0 obj", font_dict_id);
                if let Some(fd_start) = text.find(&fd_needle) {
                    let after_fd = &text[fd_start + fd_needle.len()..];
                    if let Some(fd_end) = after_fd.find("endobj") {
                        let fd_body = &after_fd[..fd_end];
                        // Parse /Name N 0 R pairs from the font dict body.
                        let chars: Vec<char> = fd_body.chars().collect();
                        let mut pos = 0;
                        while pos < chars.len() {
                            if chars[pos] == '/' {
                                let name_start = pos + 1;
                                let mut name_end = name_start;
                                while name_end < chars.len()
                                    && !chars[name_end].is_whitespace()
                                    && chars[name_end] != '/'
                                {
                                    name_end += 1;
                                }
                                let name: String = chars[name_start..name_end].iter().collect();
                                let mut p = name_end;
                                while p < chars.len() && chars[p].is_whitespace() {
                                    p += 1;
                                }
                                let ref_start = p;
                                while p < chars.len() && chars[p].is_ascii_digit() {
                                    p += 1;
                                }
                                if p > ref_start {
                                    let num_str: String = chars[ref_start..p].iter().collect();
                                    if let Ok(font_obj_id) = num_str.parse::<u32>() {
                                        let metrics =
                                            font_metrics_from_raw(doc, pdf_bytes, font_obj_id);
                                        out.insert(name, metrics);
                                    }
                                }
                                pos = p;
                            } else {
                                pos += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    out
}

/// Find the position of the matching `>>` for an opening `<<` that has been
/// consumed. Scans `s` for balanced `<<` ... `>>`.
fn find_matching_angle_brackets(s: &str) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    let mut depth = 1;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' && i + 1 < chars.len() && chars[i + 1] == '<' {
            depth += 1;
            i += 2;
            continue;
        }
        if chars[i] == '>' && i + 1 < chars.len() && chars[i + 1] == '>' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    None
}

/// Build FontMetrics for a font object by scanning raw PDF bytes for its
/// /FontFile2 stream and /Widths or /W array.
fn font_metrics_from_raw(doc: &PdfDocument, pdf_bytes: &[u8], font_obj_id: u32) -> FontMetrics {
    let text = String::from_utf8_lossy(pdf_bytes);
    let needle = format!("{} 0 obj", font_obj_id);
    let font_start = text.find(&needle);
    let font_body = if let Some(fs) = font_start {
        let after = &text[fs + needle.len()..];
        if let Some(end) = after.find("endobj") {
            &after[..end]
        } else {
            ""
        }
    } else {
        ""
    };

    // Extract embedded TTF from /FontDescriptor → /FontFile2
    let embedded_ttf = extract_embedded_ttf_raw(doc, &text, font_body);

    // Base-14 fonts carry no embedded program; use the built-in width tables
    // plus (when available) a substitute font's glyph outlines.
    if let Some(base_font) = find_name_after_key(font_body, "BaseFont")
        && is_base14_font(&base_font)
    {
        let mut metrics = base14_font_metrics(&base_font);
        attach_substitute_font(&mut metrics, &base_font);
        return metrics;
    }

    // Try to get widths from the parsed font dict (if available).
    if let Some(font_obj) = object_dict(doc, font_obj_id) {
        let mut metrics = font_metrics_for(doc, font_obj);
        metrics.embedded_ttf = metrics.embedded_ttf.or(embedded_ttf);
        if metrics.widths.is_empty() {
            // The dict parser truncates inline arrays; recover widths from raw
            // bytes. For Type0 fonts the /W array lives on the descendant CIDFont.
            let mut w = extract_widths_from_raw(doc, font_body);
            if w.is_empty()
                && let Some(df) = extract_array_text(font_body, "DescendantFonts")
                && let Some(cid_id) = df
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                && let Some(cid_body) = find_object_body(&text, cid_id)
            {
                w = extract_cid_widths_from_raw(cid_body);
            }
            metrics.widths = w;
        }
        return metrics;
    }

    // Fallback: raw widths extraction from the font body text.
    let widths = extract_widths_from_raw(doc, font_body);
    FontMetrics {
        widths,
        default_width: 500,
        embedded_ttf,
    }
}

/// Extract embedded TTF by scanning raw PDF text for /FontDescriptor → /FontFile2.
/// Also handles Type0 fonts by checking /DescendantFonts → CIDFont → /FontDescriptor.
fn extract_embedded_ttf_raw(doc: &PdfDocument, text: &str, font_body: &str) -> Option<Vec<u8>> {
    // Try /FontDescriptor directly on the font body.
    if let Some(fd_id) = find_ref_after_key(font_body, "FontDescriptor")
        && let Some(ttf) = extract_ttf_from_descriptor(doc, text, fd_id)
    {
        return Some(ttf);
    }

    // Try /DescendantFonts → [N 0 R] → CIDFont → /FontDescriptor (Type0).
    if let Some(df_str) = extract_array_text(font_body, "DescendantFonts") {
        // Parse first reference from the array
        let first_num = df_str.split_whitespace().next()?;
        let cid_id = first_num.parse::<u32>().ok()?;
        let cid_needle = format!("{} 0 obj", cid_id);
        let cid_start = text.find(&cid_needle)?;
        let after_cid = &text[cid_start + cid_needle.len()..];
        let cid_end = after_cid.find("endobj")?;
        let cid_body = &after_cid[..cid_end];

        if let Some(fd_id) = find_ref_after_key(cid_body, "FontDescriptor")
            && let Some(ttf) = extract_ttf_from_descriptor(doc, text, fd_id)
        {
            return Some(ttf);
        }
    }

    None
}

/// Extract TTF bytes from a font descriptor object via /FontFile2.
fn extract_ttf_from_descriptor(doc: &PdfDocument, text: &str, fd_id: u32) -> Option<Vec<u8>> {
    let fd_needle = format!("{} 0 obj", fd_id);
    let fd_start = text.find(&fd_needle)?;
    let after_fd = &text[fd_start + fd_needle.len()..];
    let fd_end = after_fd.find("endobj")?;
    let fd_body = &after_fd[..fd_end];

    let ff_id = find_ref_after_key(fd_body, "FontFile2")?;
    let obj = doc.objects.get(&ff_id)?;
    match obj {
        PdfObject::Stream { data, .. } => Some(decompress_stream(data)),
        _ => None,
    }
}

/// Find `N 0 R` or `N G R` after `/Key` in a PDF dict body string.
fn find_ref_after_key(body: &str, key: &str) -> Option<u32> {
    let needle = format!("/{}", key);
    let pos = body.find(&needle)?;
    let after = &body[pos + needle.len()..];
    let trimmed = after.trim_start();
    let first_num = trimmed.split_whitespace().next()?;
    first_num.parse::<u32>().ok()
}

/// Find `/Name` after `/Key` in a PDF dict body string (e.g. `/BaseFont /Courier`).
fn find_name_after_key(body: &str, key: &str) -> Option<String> {
    let needle = format!("/{}", key);
    let pos = body.find(&needle)?;
    let after = &body[pos + needle.len()..];
    let trimmed = after.trim_start();
    let name_token = trimmed.split_whitespace().next()?;
    name_token.strip_prefix('/').map(|s| s.to_string())
}

/// Extract widths from /Widths array or /W array in raw font body text.
fn extract_widths_from_raw(_doc: &PdfDocument, font_body: &str) -> HashMap<u32, u16> {
    // Try /Widths [ ... ] with /FirstChar and /LastChar
    if let (Some(fc), Some(widths_str)) = (
        find_ref_after_key(font_body, "FirstChar"),
        extract_array_text(font_body, "Widths"),
    ) {
        let widths: Vec<u16> = widths_str
            .split_whitespace()
            .filter_map(|s| s.parse::<u16>().ok())
            .collect();
        let mut map = HashMap::new();
        for (i, w) in widths.iter().enumerate() {
            map.insert(fc + i as u32, *w);
        }
        return map;
    }
    // Fallback: empty widths
    HashMap::new()
}

/// Extract the content of `[ ... ]` after `/Key` in a PDF dict body string.
fn extract_array_text(body: &str, key: &str) -> Option<String> {
    let needle = format!("/{}", key);
    let pos = body.find(&needle)?;
    let after = &body[pos + needle.len()..];
    let trimmed = after.trim_start();
    if !trimmed.starts_with('[') {
        return None;
    }
    let start = 1;
    let end = trimmed.find(']')?;
    Some(trimmed[start..end].to_string())
}

/// Locate the body of object `id` in the raw PDF text (between `N 0 obj` and `endobj`).
fn find_object_body(text: &str, id: u32) -> Option<&str> {
    let needle = format!("{} 0 obj", id);
    let start = text.find(&needle)?;
    let after = &text[start + needle.len()..];
    let end = after.find("endobj")?;
    Some(&after[..end])
}

/// Extract the balanced `[ ... ]` content (nested brackets allowed) after `/Key`.
fn extract_balanced_array_text(body: &str, key: &str) -> Option<String> {
    let needle = format!("/{}", key);
    let pos = body.find(&needle)?;
    let after = &body[pos + needle.len()..];
    let trimmed = after.trim_start();
    if !trimmed.starts_with('[') {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let mut depth = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(trimmed[1..i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a CIDFont `/W` array from raw text. Both forms are supported:
/// `c_first [w1 w2 ...]` (consecutive glyphs) and `c_first c_last w` (range).
fn extract_cid_widths_from_raw(body: &str) -> HashMap<u32, u16> {
    let mut map = HashMap::new();
    let Some(content) = extract_balanced_array_text(body, "W") else {
        return map;
    };
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let num_start = i;
        while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
            i += 1;
        }
        if num_start == i {
            i += 1;
            continue;
        }
        let Ok(c_first) = content[num_start..i].parse::<u32>() else {
            continue;
        };
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'[' {
            // Form: c [w1 w2 ...] — consecutive glyph widths.
            i += 1;
            let mut c = c_first;
            while i < bytes.len() && bytes[i] != b']' {
                while i < bytes.len() && (bytes[i] as char).is_whitespace() {
                    i += 1;
                }
                let ws = i;
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
                if ws < i
                    && let Ok(w) = content[ws..i].parse::<u16>()
                {
                    map.insert(c, w.max(1));
                    c += 1;
                }
                if ws == i {
                    i += 1;
                }
            }
            i += 1; // skip ']'
        } else {
            // Form: c_first c_last w — uniform range width.
            let mut nums = Vec::new();
            while nums.len() < 2 {
                while i < bytes.len() && (bytes[i] as char).is_whitespace() {
                    i += 1;
                }
                let ns = i;
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
                if ns == i {
                    break;
                }
                if let Ok(n) = content[ns..i].parse::<u32>() {
                    nums.push(n);
                }
            }
            if nums.len() == 2 {
                for c in c_first..=nums[0] {
                    map.insert(c, (nums[1] as u16).max(1));
                }
            }
        }
    }
    map
}

fn walk_resources(doc: &PdfDocument, val: &PdfValue, out: &mut HashMap<String, FontMetrics>) {
    let Some(dict) = (match val {
        PdfValue::Object(PdfObject::Dictionary(d)) => Some(d),
        PdfValue::Reference(id, _) => object_dict(doc, *id),
        _ => None,
    }) else {
        return;
    };
    if let Some(fonts) = dict.get("Font")
        && let Some(font_dict) = (match fonts {
            PdfValue::Object(PdfObject::Dictionary(d)) => Some(d),
            PdfValue::Reference(id, _) => object_dict(doc, *id),
            PdfValue::Object(PdfObject::String(s)) => {
                if let Some(id) = parse_ref_str(s) {
                    object_dict(doc, id)
                } else {
                    None
                }
            }
            _ => None,
        })
    {
        for (name, font_ref) in font_dict {
            let Some(font_id) = as_ref_id(font_ref) else {
                continue;
            };
            let Some(font_obj) = object_dict(doc, font_id) else {
                continue;
            };
            let metrics = font_metrics_for(doc, font_obj);
            out.insert(name.clone(), metrics);
        }
    }
    if let Some(ext_g) = dict.get("ExtGState")
        && let Some(g_dict) = match ext_g {
            PdfValue::Object(PdfObject::Dictionary(d)) => Some(d),
            PdfValue::Reference(id, _) => object_dict(doc, *id),
            _ => None,
        }
    {
        for v in g_dict.values() {
            if let Some(id) = as_ref_id(v) {
                walk_resources(doc, &PdfValue::Reference(id, 0), out);
            }
        }
    }
}

fn font_metrics_for(doc: &PdfDocument, font: &HashMap<String, PdfValue>) -> FontMetrics {
    // BaseFont name (e.g. /Helvetica-Bold)
    let base_font = match font.get("BaseFont") {
        Some(PdfValue::Object(PdfObject::Name(n))) => Some(n.clone()),
        _ => None,
    };
    let is_base14 = base_font.as_deref().map(is_base14_font).unwrap_or(false);

    if is_base14 && let Some(name) = base_font {
        let mut metrics = base14_font_metrics(&name);
        attach_substitute_font(&mut metrics, &name);
        return metrics;
    }

    // Try FirstChar/LastChar/Widths array for simple Type1 fonts
    if let (Some(fc), Some(lc), Some(widths)) = (
        font.get("FirstChar")
            .and_then(|v| as_number(doc, v))
            .map(|n| n as u32),
        font.get("LastChar")
            .and_then(|v| as_number(doc, v).map(|n| n as u32)),
        font.get("Widths").and_then(|v| as_array(doc, v)),
    ) {
        let mut map = HashMap::new();
        for (i, item) in widths.iter().enumerate() {
            if let Some(w) = as_number(doc, item) {
                let ch = fc + i as u32;
                if ch <= lc {
                    map.insert(ch, (w as u16).max(1));
                }
            }
        }
        return FontMetrics {
            widths: map,
            default_width: 500,
            embedded_ttf: extract_embedded_ttf(doc, font),
        };
    }

    // CIDFont (Type0) with /W array — used by our Unicode pipeline.
    // The /W array may be on this font dict or on a descendant CIDFont.
    let w_source = font.get("W").and_then(|v| as_array(doc, v));
    let w_source = w_source.or_else(|| {
        // For Type0 fonts, look at /DescendantFonts → [0] → /W
        let cid_id = font
            .get("DescendantFonts")
            .and_then(|v| as_array(doc, v))
            .and_then(|arr| arr.first().and_then(as_ref_id))?;
        object_dict(doc, cid_id).and_then(|cid| cid.get("W").and_then(|v| as_array(doc, v)))
    });

    if let Some(w_array) = w_source {
        let mut map = HashMap::new();
        let mut i = 0;
        while i < w_array.len() {
            // Form 1: c_first c_last width
            if i + 2 < w_array.len()
                && let (Some(c_first), Some(c_last)) = (
                    as_number(doc, &w_array[i]).map(|n| n as u32),
                    as_number(doc, &w_array[i + 1]).map(|n| n as u32),
                )
            {
                let width = as_number(doc, &w_array[i + 2]).unwrap_or(500.0) as u16;
                // Range guard: a corrupt/huge /W range must not allocate gigabytes.
                if u64::from(c_last.saturating_sub(c_first)) <= 65_535 {
                    for c in c_first..=c_last {
                        map.insert(c, width);
                    }
                }
                i += 3;
                continue;
            }
            // Form 2: c_first width [...]
            if i + 1 < w_array.len()
                && let Some(c_first) = as_number(doc, &w_array[i]).map(|n| n as u32)
            {
                let width = as_number(doc, &w_array[i + 1]).unwrap_or(500.0) as u16;
                map.insert(c_first, width);
                i += 2;
                continue;
            }
            break;
        }
        return FontMetrics {
            widths: map,
            default_width: 500,
            embedded_ttf: extract_embedded_ttf(doc, font),
        };
    }

    FontMetrics {
        widths: HashMap::new(),
        default_width: 500,
        embedded_ttf: extract_embedded_ttf(doc, font),
    }
}

/// Extract embedded TrueType font bytes from a font dictionary's /FontDescriptor → /FontFile2.
/// For Type0 fonts, resolves /DescendantFonts → CIDFont → /FontDescriptor.
fn extract_embedded_ttf(doc: &PdfDocument, font: &HashMap<String, PdfValue>) -> Option<Vec<u8>> {
    // Try /FontDescriptor directly (simple fonts).
    if let Some(desc_ref) = font.get("FontDescriptor")
        && let Some(desc_id) = as_ref_id(desc_ref)
        && let Some(desc) = object_dict(doc, desc_id)
        && let Some(file_ref) = desc.get("FontFile2")
        && let Some(file_id) = as_ref_id(file_ref)
        && let Some(PdfObject::Stream { data, .. }) = doc.objects.get(&file_id)
    {
        return Some(decompress_stream(data));
    }

    // Try /DescendantFonts → [0] → /FontDescriptor → /FontFile2 (Type0 fonts).
    if let Some(descendants) = font.get("DescendantFonts").and_then(|v| as_array(doc, v))
        && let Some(first) = descendants.first()
        && let Some(cid_id) = as_ref_id(first)
        && let Some(cid_font) = object_dict(doc, cid_id)
        && let Some(desc_ref) = cid_font.get("FontDescriptor")
        && let Some(desc_id) = as_ref_id(desc_ref)
        && let Some(desc) = object_dict(doc, desc_id)
        && let Some(file_ref) = desc.get("FontFile2")
        && let Some(file_id) = as_ref_id(file_ref)
        && let Some(PdfObject::Stream { data, .. }) = doc.objects.get(&file_id)
    {
        return Some(decompress_stream(data));
    }

    None
}
