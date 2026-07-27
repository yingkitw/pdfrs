//! Full-text search inside PDFs with per-hit bounding boxes.
//!
//! Walks each page's content stream, computes a bounding rectangle for every
//! text-show operation, and matches the query (case-insensitive substring)
//! against the visible text. Returns one [`SearchHit`] per match — enough
//! information for a viewer to scroll to and highlight the result.
//!
//! The implementation re-uses the same content-stream walker as
//! [`crate::raster`] but tracks per-character positions and emits text events
//! instead of pixels.
//!
//! ```rust,no_run
//! use pdfrs::search::search_text;
//! let pdf = std::fs::read("doc.pdf").unwrap();
//! let hits = search_text(&pdf, "needle", false);
//! for hit in &hits {
//!     println!("page {}: {:?} ({:?})", hit.page, hit.text, hit.bbox);
//! }
//! ```

use crate::pdf::{PdfDocument, PdfObject, PdfValue};
use anyhow::Result;
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

/// Search the PDF bytes for `query`.
///
/// `case_insensitive` controls substring matching: `false` matches exact case;
/// `true` folds to lowercase before matching. Returns hits in document order.
pub fn search_text(pdf_bytes: &[u8], query: &str, case_insensitive: bool) -> Vec<SearchHit> {
    let doc = match PdfDocument::load_from_bytes(pdf_bytes) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let pages = collect_pages_from_doc(&doc, Some(pdf_bytes));
    let fonts = collect_font_metrics(&doc);

    let needle = if case_insensitive {
        query.to_lowercase()
    } else {
        query.to_string()
    };
    if needle.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    for (page_idx, page_id) in pages.iter().enumerate() {
        let content_ids = match page_content_streams(&doc, *page_id) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let mut collector = HitCollector::new(page_idx, &needle, &fonts, case_insensitive);
        for cid in content_ids {
            let raw = match doc.objects.get(&cid) {
                Some(PdfObject::Stream { data, .. }) => data.clone(),
                _ => continue,
            };
            let decompressed = decompress_stream(&raw);
            let text = String::from_utf8_lossy(&decompressed).into_owned();
            walk_content_stream(&text, &mut collector);
        }
        hits.extend(collector.into_hits());
    }
    hits
}

/// Count hits without returning individual rectangles (cheap pre-check).
pub fn count_matches(pdf_bytes: &[u8], query: &str, case_insensitive: bool) -> usize {
    search_text(pdf_bytes, query, case_insensitive).len()
}

// ----- Page collection ----------------------------------------------------

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

fn page_content_streams(doc: &PdfDocument, page_id: u32) -> Result<Vec<u32>> {
    let dict = object_dict(doc, page_id)
        .ok_or_else(|| anyhow::anyhow!("page {page_id} not a dictionary"))?;
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

pub(crate) fn object_dict<'a>(
    doc: &'a PdfDocument,
    id: u32,
) -> Option<&'a HashMap<String, PdfValue>> {
    doc.objects.get(&id).and_then(|o| match o {
        PdfObject::Dictionary(d) => Some(d),
        PdfObject::Stream { dictionary, .. } => Some(dictionary),
        _ => None,
    })
}

fn as_array(doc: &PdfDocument, val: &PdfValue) -> Option<Vec<PdfValue>> {
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
    if data.len() > 2 && data[0] == 0x78 {
        crate::compression::decompress_deflate(data).unwrap_or_else(|_| data.to_vec())
    } else {
        data.to_vec()
    }
}

/// Best-effort detection of zlib/FlateDecode-compressed stream bytes.
pub(crate) fn is_deflate_stream(data: &[u8]) -> bool {
    data.len() > 2 && data[0] == 0x78 && crate::compression::decompress_deflate(data).is_ok()
}

// ----- Font metrics -------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FontMetrics {
    widths: HashMap<u32, u16>,
    default_width: u16,
}

impl FontMetrics {
    pub fn advance(&self, ch: u32) -> u16 {
        self.widths.get(&ch).copied().unwrap_or(self.default_width)
    }
}

pub fn collect_font_metrics(doc: &PdfDocument) -> HashMap<String, FontMetrics> {
    let mut out = HashMap::new();
    let mut queue: Vec<u32> = vec![doc.catalog];
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = queue.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(dict) = object_dict(doc, id)
            && let Some(resources) = dict.get("Resources")
        {
            walk_resources(doc, resources, &mut out);
        }
    }
    out
}

fn walk_resources(doc: &PdfDocument, val: &PdfValue, out: &mut HashMap<String, FontMetrics>) {
    let Some(dict) = (match val {
        PdfValue::Object(PdfObject::Dictionary(d)) => Some(d),
        PdfValue::Reference(id, _) => object_dict(doc, *id),
        _ => None,
    }) else {
        return;
    };
    if let Some(fonts) = dict.get("Font") {
        let font_dict = match fonts {
            PdfValue::Object(PdfObject::Dictionary(d)) => Some(d),
            PdfValue::Reference(id, _) => object_dict(doc, *id),
            _ => None,
        };
        if let Some(font_dict) = font_dict {
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
    }
}

fn font_metrics_for(doc: &PdfDocument, font: &HashMap<String, PdfValue>) -> FontMetrics {
    let base_font = match font.get("BaseFont") {
        Some(v) => match v {
            PdfValue::Object(PdfObject::Name(n)) => Some(n.clone()),
            PdfValue::Object(PdfObject::String(s)) => Some(s.trim_start_matches('/').to_string()),
            _ => None,
        },
        None => None,
    };
    let is_base14 = base_font.as_deref().map(is_base14_font).unwrap_or(false);
    if is_base14 && let Some(name) = base_font {
        return base14_font_metrics(&name);
    }
    // Widths[] for simple Type1 fonts
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
        };
    }
    // CIDFont /W array
    if let Some(w_array) = font.get("W").and_then(|v| as_array(doc, v)) {
        let mut map = HashMap::new();
        let mut i = 0;
        while i < w_array.len() {
            if i + 2 < w_array.len()
                && let (Some(c_first), Some(c_last)) = (
                    as_number(doc, &w_array[i]).map(|n| n as u32),
                    as_number(doc, &w_array[i + 1]).map(|n| n as u32),
                )
            {
                let width = as_number(doc, &w_array[i + 2]).unwrap_or(500.0) as u16;
                for c in c_first..=c_last {
                    map.insert(c, width);
                }
                i += 3;
                continue;
            }
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
        };
    }
    FontMetrics {
        widths: HashMap::new(),
        default_width: 500,
    }
}

fn as_number(doc: &PdfDocument, val: &PdfValue) -> Option<f32> {
    match val {
        PdfValue::Object(PdfObject::Number(n)) => Some(*n as f32),
        PdfValue::Reference(id, _) => doc.objects.get(id).and_then(|o| {
            if let PdfObject::Number(n) = o {
                Some(*n as f32)
            } else {
                None
            }
        }),
        PdfValue::Object(PdfObject::String(s)) => {
            let trimmed = s.trim().trim_matches(|c| c == '[' || c == ']');
            trimmed
                .split_whitespace()
                .next()
                .and_then(|t| t.parse::<f32>().ok())
        }
        _ => None,
    }
}

fn is_base14_font(name: &str) -> bool {
    matches!(
        name,
        "Helvetica"
            | "Helvetica-Bold"
            | "Helvetica-Oblique"
            | "Helvetica-BoldOblique"
            | "Times-Roman"
            | "Times-Bold"
            | "Times-Italic"
            | "Times-BoldItalic"
            | "Courier"
            | "Courier-Bold"
            | "Courier-Oblique"
            | "Courier-BoldOblique"
            | "Symbol"
            | "ZapfDingbats"
    )
}

fn base14_font_metrics(name: &str) -> FontMetrics {
    let widths = match name {
        "Helvetica" | "Helvetica-Bold" | "Helvetica-Oblique" | "Helvetica-BoldOblique" => {
            helvetica_widths()
        }
        "Times-Roman" | "Times-Bold" | "Times-Italic" | "Times-BoldItalic" => times_widths(),
        "Courier" | "Courier-Bold" | "Courier-Oblique" | "Courier-BoldOblique" => courier_widths(),
        _ => HashMap::new(),
    };
    FontMetrics {
        widths,
        default_width: 500,
    }
}

fn helvetica_widths() -> HashMap<u32, u16> {
    let raw: &[(u32, u16)] = &[
        (32, 278),
        (33, 278),
        (34, 355),
        (35, 556),
        (36, 556),
        (37, 889),
        (38, 667),
        (39, 191),
        (40, 333),
        (41, 333),
        (42, 389),
        (43, 584),
        (44, 278),
        (45, 333),
        (46, 278),
        (47, 278),
        (48, 556),
        (49, 556),
        (50, 556),
        (51, 556),
        (52, 556),
        (53, 556),
        (54, 556),
        (55, 556),
        (56, 556),
        (57, 556),
        (58, 278),
        (59, 278),
        (60, 584),
        (61, 584),
        (62, 584),
        (63, 556),
        (64, 1015),
        (65, 667),
        (66, 667),
        (67, 722),
        (68, 722),
        (69, 667),
        (70, 611),
        (71, 778),
        (72, 722),
        (73, 278),
        (74, 500),
        (75, 667),
        (76, 556),
        (77, 833),
        (78, 722),
        (79, 778),
        (80, 667),
        (81, 778),
        (82, 722),
        (83, 667),
        (84, 611),
        (85, 722),
        (86, 667),
        (87, 944),
        (88, 667),
        (89, 667),
        (90, 611),
        (97, 556),
        (98, 556),
        (99, 500),
        (100, 556),
        (101, 556),
        (102, 278),
        (103, 556),
        (104, 556),
        (105, 222),
        (106, 222),
        (107, 500),
        (108, 222),
        (109, 833),
        (110, 556),
        (111, 556),
        (112, 556),
        (113, 556),
        (114, 333),
        (115, 500),
        (116, 278),
        (117, 556),
        (118, 500),
        (119, 722),
        (120, 500),
        (121, 500),
        (122, 500),
    ];
    raw.iter().copied().collect()
}

fn times_widths() -> HashMap<u32, u16> {
    let raw: &[(u32, u16)] = &[
        (32, 250),
        (33, 333),
        (34, 408),
        (35, 500),
        (36, 500),
        (37, 833),
        (38, 778),
        (39, 180),
        (40, 333),
        (41, 333),
        (42, 500),
        (43, 564),
        (44, 250),
        (45, 333),
        (46, 250),
        (47, 278),
        (48, 500),
        (49, 500),
        (50, 500),
        (51, 500),
        (52, 500),
        (53, 500),
        (54, 500),
        (55, 500),
        (56, 500),
        (57, 500),
        (58, 278),
        (59, 278),
        (60, 564),
        (61, 564),
        (62, 564),
        (63, 444),
        (64, 921),
        (65, 722),
        (66, 667),
        (67, 667),
        (68, 722),
        (69, 611),
        (70, 556),
        (71, 722),
        (72, 722),
        (73, 333),
        (74, 389),
        (75, 722),
        (76, 611),
        (77, 889),
        (78, 722),
        (79, 722),
        (80, 556),
        (81, 722),
        (82, 667),
        (83, 556),
        (84, 611),
        (85, 722),
        (86, 722),
        (87, 944),
        (88, 722),
        (89, 722),
        (90, 611),
        (97, 444),
        (98, 500),
        (99, 444),
        (100, 500),
        (101, 444),
        (102, 333),
        (103, 500),
        (104, 500),
        (105, 278),
        (106, 278),
        (107, 500),
        (108, 278),
        (109, 778),
        (110, 500),
        (111, 500),
        (112, 500),
        (113, 500),
        (114, 333),
        (115, 389),
        (116, 278),
        (117, 500),
        (118, 500),
        (119, 722),
        (120, 500),
        (121, 500),
        (122, 444),
    ];
    raw.iter().copied().collect()
}

fn courier_widths() -> HashMap<u32, u16> {
    let mut out = HashMap::new();
    for c in 32..=127 {
        out.insert(c, 600);
    }
    out
}

// ----- Content-stream walking --------------------------------------------

/// Receives text spans as the walker encounters them.
struct HitCollector<'a> {
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
    fn new(
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

    fn into_hits(mut self) -> Vec<SearchHit> {
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
        if &hchars[i..i + nchars.len()] == &nchars[..] {
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

fn walk_content_stream(src: &str, collector: &mut HitCollector) {
    let tokens = tokenize(src);
    let mut i = 0;
    let mut operands: Vec<f32> = Vec::new();
    let mut text_matrix = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut text_line_matrix = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut font_size = 12.0f32;
    while i < tokens.len() {
        let t = &tokens[i];
        if let Some(n) = t.parse::<f32>().ok() {
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
                if let Some(text) = extract_string(&tokens, i) {
                    let x = text_matrix[4];
                    let y = text_matrix[5];
                    collector.push_text(&text, x, y, font_size);
                    let advance = collector.boxes.last().map(|b| b.2).unwrap_or(0.0);
                    text_matrix[4] = x + advance;
                }
            }
            "TJ" => {
                if let Some(items) = extract_tj_array(&tokens, i) {
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
                if let Some(text) = extract_string(&tokens, i) {
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
            let decoded = decode_hex_string(&prev[start..end]);
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
        Some(decode_hex_string(inner))
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

fn decode_hex_string(raw: &str) -> String {
    let hex: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| {
            if i + 1 < hex.len() {
                u8::from_str_radix(&hex[i..i + 2], 16).ok()
            } else if i < hex.len() {
                u8::from_str_radix(&format!("{}0", &hex[i..i + 1]), 16).ok()
            } else {
                None
            }
        })
        .collect();
    // UTF-16BE BOM (FE FF) → decode as UTF-16
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        return decode_utf16be(&bytes[2..]);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn decode_utf16be(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let cu = u16::from_be_bytes([bytes[i], bytes[i + 1]]);
        if let Some(c) = char::from_u32(cu as u32) {
            out.push(c);
        }
        i += 2;
    }
    out
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
    fn finds_single_word() {
        let pdf = make_pdf("# Hello world\n\nThis is pdfrs.");
        let hits = search_text(&pdf, "pdfrs", false);
        assert_eq!(hits.len(), 1, "{:?}", hits);
        assert_eq!(hits[0].text, "pdfrs");
        assert_eq!(hits[0].page, 0);
        assert!(hits[0].bbox.width > 0.0);
        assert!(hits[0].bbox.height > 0.0);
    }

    #[test]
    fn case_insensitive_match() {
        let pdf = make_pdf("# HELLO\n\nworld");
        assert_eq!(search_text(&pdf, "hello", false).len(), 0);
        assert_eq!(search_text(&pdf, "hello", true).len(), 1);
    }

    #[test]
    fn multiple_hits_same_query() {
        let pdf = make_pdf("# foo bar foo\n\nfoo again");
        let hits = search_text(&pdf, "foo", false);
        assert!(hits.len() >= 3, "{} hits", hits.len());
        for hit in &hits {
            assert_eq!(hit.text, "foo");
        }
    }

    #[test]
    fn empty_query_returns_no_hits() {
        let pdf = make_pdf("anything");
        assert!(search_text(&pdf, "", false).is_empty());
    }

    #[test]
    fn count_matches_short_circuits() {
        let pdf = make_pdf("# foo bar foo");
        assert_eq!(count_matches(&pdf, "foo", false), 2);
    }

    #[test]
    fn snippet_contains_match() {
        let pdf = make_pdf("# The quick brown fox jumps over the lazy dog");
        let hits = search_text(&pdf, "fox", false);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("fox"));
    }

    #[test]
    fn rect_intersection() {
        let a = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        let b = Rect {
            x: 5.0,
            y: 5.0,
            width: 10.0,
            height: 10.0,
        };
        assert!(a.intersects(&b));
        let c = Rect {
            x: 100.0,
            y: 100.0,
            width: 5.0,
            height: 5.0,
        };
        assert!(!a.intersects(&c));
    }
}
