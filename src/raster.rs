//! Native PDF page rasterization (PDF → PNG), pure Rust, no external dependencies.
//!
//! This is a **schematic** rasterizer: graphics (rectangles, lines, ellipses,
//! polygons, Bézier paths) are rendered faithfully to a pixel buffer; text is
//! rendered as light-gray glyph-block rectangles positioned using standard PDF
//! font width tables (PDF 32000-1 base-14 widths) or — when an embedded
//! TrueType font is present — actual glyph advances via `ttf-parser`.
//!
//! It produces layout-faithful PNG previews without depending on pdf.js,
//! Ghostscript, PDFium, or any external font rasterizer. The PNG encoder is
//! implemented inline (deflate IDAT chunks via [`flate2`]).
//!
//! ## Scope
//!
//! Renders the operators emitted by `pdfrs` itself plus the common PDF
//! content-stream subset used by simpler producers:
//!
//! | Operator | Meaning | Supported |
//! |---|---|---|
//! | `q` / `Q` | Save / restore graphics state | ✅ |
//! | `cm` | Concatenate matrix | ✅ |
//! | `w` | Line width | ✅ |
//! | `rg` / `RG` / `g` / `G` / `k` / `K` | Color | ✅ |
//! | `m` / `l` / `c` / `h` / `re` | Path construction | ✅ |
//! | `S` / `s` / `f` / `B` / `b` / `n` | Path painting | ✅ |
//! | `BT` / `ET` | Text object | ✅ |
//! | `Tf` | Text font/size | ✅ |
//! | `Tm` / `Td` / `TD` / `T*` | Text matrix / position | ✅ |
//! | `Tj` / `TJ` / `'` / `"` | Show text | ✅ |
//!
//! ## Limits
//!
//! - When an embedded TrueType font is available, glyph **outlines** are
//!   rasterised via `ttf-parser` for crisp, shape-accurate text rendering.
//!   Base-14 fonts (no embedded data) fall back to gray glyph-block rectangles.
//! - Type 3 fonts, patterns, shadings, transparency groups, and images are
//!   not yet rendered.
//!
//! [`flate2`]: https://docs.rs/flate2

use crate::pdf::{PdfDocument, PdfObject, PdfValue};
use crate::search::{decompress_stream, page_content_streams};
use anyhow::{Result, anyhow};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::collections::HashMap;
use std::io::Write;

/// A rasterised page.
#[derive(Debug, Clone)]
pub struct RasterPage {
    /// RGBA8 pixel buffer, top-left origin.
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl RasterPage {
    /// Encode the page as a PNG byte string.
    pub fn to_png(&self) -> Result<Vec<u8>> {
        encode_png(self.width, self.height, &self.pixels)
    }

    /// Convenience: encode and write to `path`.
    pub fn write_png(&self, path: &str) -> Result<()> {
        let png = self.to_png()?;
        std::fs::write(path, png)?;
        Ok(())
    }
}

/// Rasterise a single PDF page (0-indexed) to an RGBA pixel buffer at `dpi`.
///
/// Page size honours the page's `/MediaBox` (in PDF points; 72 pt = 1 in).
pub fn rasterize_page(pdf_bytes: &[u8], page_index: usize, dpi: u32) -> Result<RasterPage> {
    let doc = PdfDocument::load_from_bytes(pdf_bytes)?;
    let pages = crate::search::collect_pages_from_doc(&doc, Some(pdf_bytes));
    let page_id = *pages.get(page_index).ok_or_else(|| {
        anyhow!(
            "page index {page_index} out of range ({} pages)",
            pages.len()
        )
    })?;

    let (width_pt, height_pt) = page_media_box(pdf_bytes, &doc, page_id)?;
    let scale = (dpi as f32) / 72.0;
    let width_px = ((width_pt * scale).round() as u32).max(1);
    let height_px = ((height_pt * scale).round() as u32).max(1);

    let content_ids = page_content_streams(&doc, page_id)?;
    let font_metrics = collect_font_metrics_with_raw(&doc, pdf_bytes, page_id);

    let mut surface = Surface::new(width_px, height_px);
    // PDF origin is bottom-left; flip Y so we can paint top-down.
    surface.transform = [scale, 0.0, 0.0, -scale, 0.0, height_pt * scale];

    for cid in content_ids {
        let raw = match doc.objects.get(&cid) {
            Some(PdfObject::Stream { data, .. }) => data.clone(),
            _ => continue,
        };
        let decompressed = decompress_stream(&raw);
        let text = String::from_utf8_lossy(&decompressed).into_owned();
        render_content_stream(&mut surface, &text, &font_metrics);
    }

    Ok(RasterPage {
        pixels: surface.pixels,
        width: width_px,
        height: height_px,
    })
}

/// Rasterise every page in the PDF.
///
/// Returns one `RasterPage` per page in document order.
pub fn rasterize_all(pdf_bytes: &[u8], dpi: u32) -> Result<Vec<RasterPage>> {
    let doc = PdfDocument::load_from_bytes(pdf_bytes)?;
    let pages = crate::search::collect_pages_from_doc(&doc, Some(pdf_bytes));
    let mut out = Vec::with_capacity(pages.len());
    for (i, _) in pages.iter().enumerate() {
        out.push(rasterize_page(pdf_bytes, i, dpi)?);
    }
    Ok(out)
}

/// Returns `(width_pt, height_pt)` of a page's MediaBox.
fn page_media_box(pdf_bytes: &[u8], doc: &PdfDocument, page_id: u32) -> Result<(f32, f32)> {
    // Scan the raw PDF for the first /MediaBox inside the page object body.
    // The bundled dict parser truncates bracket arrays at the first whitespace,
    // so we go straight to the source text.
    if let Some(values) = raw_mediabox(pdf_bytes, page_id)
        && values.len() >= 4
    {
        return Ok((values[2] - values[0], values[3] - values[1]));
    }

    // Fallback to the parsed dict (in case the PDF was already structured).
    let dict =
        object_dict(doc, page_id).ok_or_else(|| anyhow!("page {page_id} not a dictionary"))?;
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

fn object_dict(doc: &PdfDocument, id: u32) -> Option<&HashMap<String, PdfValue>> {
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
        _ => None,
    }
}

fn as_ref_id(val: &PdfValue) -> Option<u32> {
    match val {
        PdfValue::Reference(id, _) => Some(*id),
        PdfValue::Object(PdfObject::Reference(id, _)) => Some(*id),
        PdfValue::Object(PdfObject::String(s)) => parse_ref_str(s),
        _ => None,
    }
}

/// Parse an `N G R` reference literal, a bare numeric id, or a `[N]` bracket
/// literal out of a string. Returns the first parseable object id.
fn parse_ref_str(s: &str) -> Option<u32> {
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

// ----- Font metrics -------------------------------------------------------

#[derive(Debug, Clone)]
struct FontMetrics {
    /// Width of every char in 1/1000 em (font units).
    widths: HashMap<u32, u16>,
    /// Default width when a char isn't in the table.
    default_width: u16,
    /// Raw embedded TrueType font bytes (from /FontFile2), if present.
    embedded_ttf: Option<Vec<u8>>,
}

impl FontMetrics {
    fn advance(&self, ch: u32) -> u16 {
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
fn collect_font_metrics_with_raw(
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
        // Case 2: /Font N 0 R — font dict is an indirect object
        else {
            let trimmed = after_font.trim_start();
            if let Some(first_num) = trimmed.split_whitespace().next()
                && let Ok(font_dict_id) = first_num.parse::<u32>() {
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
                                            let metrics = font_metrics_from_raw(
                                                doc, pdf_bytes, font_obj_id,
                                            );
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
fn font_metrics_from_raw(
    doc: &PdfDocument,
    pdf_bytes: &[u8],
    font_obj_id: u32,
) -> FontMetrics {
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

    // Base-14 fonts carry no embedded program; use the built-in width tables.
    if let Some(base_font) = find_name_after_key(font_body, "BaseFont")
        && is_base14_font(&base_font)
    {
        return base14_font_metrics(&base_font);
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
                && let Some(cid_id) = df.split_whitespace().next().and_then(|s| s.parse::<u32>().ok())
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
fn extract_embedded_ttf_raw(
    doc: &PdfDocument,
    text: &str,
    font_body: &str,
) -> Option<Vec<u8>> {
    // Try /FontDescriptor directly on the font body.
    if let Some(fd_id) = find_ref_after_key(font_body, "FontDescriptor")
        && let Some(ttf) = extract_ttf_from_descriptor(doc, text, fd_id) {
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
            && let Some(ttf) = extract_ttf_from_descriptor(doc, text, fd_id) {
                return Some(ttf);
            }
    }

    None
}

/// Extract TTF bytes from a font descriptor object via /FontFile2.
fn extract_ttf_from_descriptor(
    doc: &PdfDocument,
    text: &str,
    fd_id: u32,
) -> Option<Vec<u8>> {
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
        return base14_font_metrics(&name);
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
        object_dict(doc, cid_id)
            .and_then(|cid| cid.get("W").and_then(|v| as_array(doc, v)))
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
                for c in c_first..=c_last {
                    map.insert(c, width);
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
                        && let Some(PdfObject::Stream { data, .. }) = doc.objects.get(&file_id) {
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
                                        && let Some(PdfObject::Stream { data, .. }) =
                                            doc.objects.get(&file_id)
                                        {
                                            return Some(decompress_stream(data));
                                        }

    None
}

// ----- Surface & rendering ------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
}

impl Color {
    const BLACK: Color = Color { r: 0, g: 0, b: 0 };

    fn from_gray(v: f32) -> Self {
        let c = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color { r: c, g: c, b: c }
    }
    fn from_rgb(r: f32, g: f32, b: f32) -> Self {
        Color {
            r: (r.clamp(0.0, 1.0) * 255.0).round() as u8,
            g: (g.clamp(0.0, 1.0) * 255.0).round() as u8,
            b: (b.clamp(0.0, 1.0) * 255.0).round() as u8,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct State {
    fill: Color,
    stroke: Color,
    line_width: f32,
}

impl Default for State {
    fn default() -> Self {
        State {
            fill: Color::BLACK,
            stroke: Color::BLACK,
            line_width: 1.0,
        }
    }
}

struct Surface {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    /// CTM: [a, b, c, d, e, f] in PDF coords (origin bottom-left).
    transform: [f32; 6],
    state_stack: Vec<State>,
    state: State,
    /// Path under construction, in PDF user space.
    path: Vec<PathSegment>,
    subpath_start: (f32, f32),
    current_point: (f32, f32),
    /// True while inside BT..ET (text object mode).
    in_text: bool,
    /// Text matrix [a, b, c, d, e, f].
    text_matrix: [f32; 6],
    /// Text line matrix (for T*)
    text_line_matrix: [f32; 6],
    /// Current font size in points.
    font_size: f32,
    /// Current font metrics (by font name, e.g. "F1").
    font_metrics: Option<FontMetrics>,
}

#[derive(Debug, Clone, Copy)]
enum PathSegment {
    Move(f32, f32),
    Line(f32, f32),
    Curve(f32, f32, f32, f32, f32, f32),
    Close,
}

impl Surface {
    fn new(width: u32, height: u32) -> Self {
        let pixels = vec![255u8; (width as usize) * (height as usize) * 4];
        Surface {
            pixels,
            width,
            height,
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            state_stack: Vec::new(),
            state: State::default(),
            path: Vec::new(),
            subpath_start: (0.0, 0.0),
            current_point: (0.0, 0.0),
            in_text: false,
            text_matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            text_line_matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            font_size: 12.0,
            font_metrics: None,
        }
    }

    fn set_pixel(&mut self, x: u32, y: u32, color: Color, alpha: f32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = ((y * self.width + x) as usize) * 4;
        let a = alpha.clamp(0.0, 1.0);
        if a >= 1.0 {
            self.pixels[i] = color.r;
            self.pixels[i + 1] = color.g;
            self.pixels[i + 2] = color.b;
            self.pixels[i + 3] = 255;
        } else {
            let inv = 1.0 - a;
            self.pixels[i] = (color.r as f32 * a + self.pixels[i] as f32 * inv) as u8;
            self.pixels[i + 1] = (color.g as f32 * a + self.pixels[i + 1] as f32 * inv) as u8;
            self.pixels[i + 2] = (color.b as f32 * a + self.pixels[i + 2] as f32 * inv) as u8;
            self.pixels[i + 3] = 255;
        }
    }

    /// Transform PDF (x, y) by the current CTM, returning screen coords (px, py).
    fn transform_pt(&self, x: f32, y: f32) -> (f32, f32) {
        let m = self.transform;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }

    fn stroke_path(&mut self) {
        if self.path.is_empty() {
            return;
        }
        // Flatten to line segments, then stroke.
        let mut segments: Vec<(f32, f32, f32, f32)> = Vec::new();
        let mut start = self.subpath_start;
        let mut cur = self.current_point;
        for seg in &self.path {
            match seg {
                PathSegment::Move(x, y) => {
                    start = (*x, *y);
                    cur = (*x, *y);
                }
                PathSegment::Line(x, y) => {
                    segments.push((cur.0, cur.1, *x, *y));
                    cur = (*x, *y);
                }
                PathSegment::Curve(x1, y1, x2, y2, x3, y3) => {
                    flatten_cubic_into_segments(
                        &mut segments,
                        cur,
                        (*x1, *y1),
                        (*x2, *y2),
                        (*x3, *y3),
                    );
                    cur = (*x3, *y3);
                }
                PathSegment::Close => {
                    segments.push((cur.0, cur.1, start.0, start.1));
                    cur = start;
                }
            }
        }
        let lw = self.state.line_width.max(0.5);
        for (a, b, c, d) in segments {
            self.draw_line(a, b, c, d, lw);
        }
    }

    fn fill_path(&mut self) {
        if self.path.is_empty() {
            return;
        }
        // Build a flattened polygon of the current subpath(s).
        let mut polys: Vec<Vec<(f32, f32)>> = Vec::new();
        let mut current: Vec<(f32, f32)> = Vec::new();
        let mut start = self.subpath_start;
        let mut cur = self.current_point;
        for seg in &self.path {
            match seg {
                PathSegment::Move(x, y) => {
                    if !current.is_empty() {
                        polys.push(std::mem::take(&mut current));
                    }
                    start = (*x, *y);
                    current.push((*x, *y));
                    cur = (*x, *y);
                }
                PathSegment::Line(x, y) => {
                    current.push((*x, *y));
                    cur = (*x, *y);
                }
                PathSegment::Curve(x1, y1, x2, y2, x3, y3) => {
                    flatten_cubic_into(&mut current, cur, (*x1, *y1), (*x2, *y2), (*x3, *y3));
                    cur = (*x3, *y3);
                }
                PathSegment::Close => {
                    if !current.is_empty() {
                        polys.push(std::mem::take(&mut current));
                    }
                    cur = start;
                }
            }
        }
        if !current.is_empty() {
            polys.push(current);
        }
        for poly in polys {
            self.fill_polygon(&poly);
        }
    }

    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, lw: f32) {
        let (sx1, sy1) = self.transform_pt(x1, y1);
        let (sx2, sy2) = self.transform_pt(x2, y2);
        let color = self.state.stroke;
        let radius =
            (lw * 0.5 * (self.transform[0].abs() + self.transform[3].abs()) * 0.5).max(0.5);

        // Use Bresenham-style with a thin brush radius for anti-aliased strokes.
        let steps = ((sx2 - sx1).hypot(sy2 - sy1).ceil() as i32 + 2).max(2);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let px = sx1 + (sx2 - sx1) * t;
            let py = sy1 + (sy2 - sy1) * t;
            self.disc(px.round() as i32, py.round() as i32, radius, color);
        }
    }

    fn disc(&mut self, cx: i32, cy: i32, r: f32, color: Color) {
        let r2 = r.ceil() as i32 + 1;
        let r_sq = r * r;
        for dy in -r2..=r2 {
            for dx in -r2..=r2 {
                let dist_sq = (dx * dx + dy * dy) as f32;
                if dist_sq <= r_sq {
                    let alpha = 1.0 - (dist_sq.sqrt() / r).clamp(0.0, 1.0);
                    let x = (cx + dx) as u32;
                    let y = (cy + dy) as u32;
                    self.set_pixel(x, y, color, alpha);
                }
            }
        }
    }

    fn fill_polygon(&mut self, poly: &[(f32, f32)]) {
        if poly.len() < 3 {
            return;
        }
        // Transform to screen coords.
        let screen: Vec<(f32, f32)> = poly.iter().map(|&(x, y)| self.transform_pt(x, y)).collect();
        let color = self.state.fill;
        let mut min_y = screen[0].1;
        let mut max_y = screen[0].1;
        for p in &screen {
            if p.1 < min_y {
                min_y = p.1;
            }
            if p.1 > max_y {
                max_y = p.1;
            }
        }
        let y_start = (min_y.floor() as i32).max(0);
        let y_end = (max_y.ceil() as i32).min(self.height as i32 - 1);
        for y in y_start..=y_end {
            let yf = y as f32 + 0.5;
            let mut xs: Vec<f32> = Vec::new();
            let n = screen.len();
            for i in 0..n {
                let (x1, y1) = screen[i];
                let (x2, y2) = screen[(i + 1) % n];
                if (y1 <= yf && y2 > yf) || (y2 <= yf && y1 > yf) {
                    let t = (yf - y1) / (y2 - y1);
                    xs.push(x1 + (x2 - x1) * t);
                }
            }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mut i = 0;
            while i + 1 < xs.len() {
                let xa = xs[i];
                let xb = xs[i + 1];
                let x_start = (xa.ceil() as i32).max(0);
                let x_end = (xb.floor() as i32).min(self.width as i32 - 1);
                if x_end >= x_start {
                    for x in x_start..=x_end {
                        self.set_pixel(x as u32, y as u32, color, 1.0);
                    }
                }
                i += 2;
            }
        }
    }
}

fn flatten_cubic_into(
    poly: &mut Vec<(f32, f32)>,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
) {
    flatten_cubic_points(poly, p0, p1, p2, p3);
}

// Tessellate cubic Bezier into segments/points via midpoint subdivision.
fn flatten_cubic_into_segments(
    out: &mut Vec<(f32, f32, f32, f32)>,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
) {
    let mut stack = vec![(p0, p1, p2, p3)];
    while let Some((a, b, c, d)) = stack.pop() {
        let chord = (d.0 - a.0).hypot(d.1 - a.1);
        let poly1 = (b.0 - a.0).hypot(b.1 - a.1);
        let poly2 = (c.0 - d.0).hypot(c.1 - d.1);
        let total = poly1 + poly2;
        if chord < 0.5 || total - chord < 0.5 || (a == b && c == d) {
            out.push((a.0, a.1, d.0, d.1));
        } else {
            let m01 = midpoint(a, b);
            let m12 = midpoint(b, c);
            let m23 = midpoint(c, d);
            let m012 = midpoint(m01, m12);
            let m123 = midpoint(m12, m23);
            let m0123 = midpoint(m012, m123);
            stack.push((a, m01, m012, m0123));
            stack.push((m0123, m123, m23, d));
        }
    }
}

fn flatten_cubic_points(
    poly: &mut Vec<(f32, f32)>,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
) {
    let mut stack = vec![(p0, p1, p2, p3)];
    let mut started = false;
    while let Some((a, b, c, d)) = stack.pop() {
        let chord = (d.0 - a.0).hypot(d.1 - a.1);
        let poly1 = (b.0 - a.0).hypot(b.1 - a.1);
        let poly2 = (c.0 - d.0).hypot(c.1 - d.1);
        let total = poly1 + poly2;
        if chord < 0.5 || total - chord < 0.5 {
            if !started {
                poly.push(a);
                started = true;
            }
            poly.push(d);
        } else {
            let m01 = midpoint(a, b);
            let m12 = midpoint(b, c);
            let m23 = midpoint(c, d);
            let m012 = midpoint(m01, m12);
            let m123 = midpoint(m12, m23);
            let m0123 = midpoint(m012, m123);
            stack.push((a, m01, m012, m0123));
            stack.push((m0123, m123, m23, d));
        }
    }
}

fn midpoint(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
}

// ----- Content-stream parsing & dispatch ----------------------------------

fn render_content_stream(surface: &mut Surface, src: &str, fonts: &HashMap<String, FontMetrics>) {
    let tokens = tokenize(src);
    let mut i = 0;
    let mut operands: Vec<f32> = Vec::new();
    while i < tokens.len() {
        let t = &tokens[i];
        if let Some(n) = parse_number(t) {
            operands.push(n);
            i += 1;
            continue;
        }
        let op = t.as_str();
        match op {
            "q" => {
                surface.state_stack.push(surface.state);
            }
            "Q" => {
                if let Some(state) = surface.state_stack.pop() {
                    surface.state = state;
                }
            }
            "cm" => {
                if operands.len() == 6 {
                    let m = surface.transform;
                    let n = [
                        operands[0],
                        operands[1],
                        operands[2],
                        operands[3],
                        operands[4],
                        operands[5],
                    ];
                    surface.transform = [
                        m[0] * n[0] + m[2] * n[1],
                        m[1] * n[0] + m[3] * n[1],
                        m[0] * n[2] + m[2] * n[3],
                        m[1] * n[2] + m[3] * n[3],
                        m[0] * n[4] + m[2] * n[5] + m[4],
                        m[1] * n[4] + m[3] * n[5] + m[5],
                    ];
                }
            }
            "w" => {
                if let Some(v) = operands.first() {
                    surface.state.line_width = *v;
                }
            }
            "rg" => {
                if operands.len() == 3 {
                    surface.state.fill = Color::from_rgb(operands[0], operands[1], operands[2]);
                }
            }
            "RG" => {
                if operands.len() == 3 {
                    surface.state.stroke = Color::from_rgb(operands[0], operands[1], operands[2]);
                }
            }
            "g" => {
                if let Some(v) = operands.first() {
                    surface.state.fill = Color::from_gray(*v);
                }
            }
            "G" => {
                if let Some(v) = operands.first() {
                    surface.state.stroke = Color::from_gray(*v);
                }
            }
            "k" | "K" => {
                if operands.len() == 4 {
                    let c = cmyk_to_rgb(operands[0], operands[1], operands[2], operands[3]);
                    if op == "k" {
                        surface.state.fill = c;
                    } else {
                        surface.state.stroke = c;
                    }
                }
            }
            "m" => {
                if operands.len() >= 2 {
                    let x = operands[operands.len() - 2];
                    let y = operands[operands.len() - 1];
                    surface.path.push(PathSegment::Move(x, y));
                    surface.subpath_start = (x, y);
                    surface.current_point = (x, y);
                }
            }
            "l" => {
                if operands.len() >= 2 {
                    let x = operands[operands.len() - 2];
                    let y = operands[operands.len() - 1];
                    surface.path.push(PathSegment::Line(x, y));
                    surface.current_point = (x, y);
                }
            }
            "c" => {
                if operands.len() >= 6 {
                    let n = operands.len();
                    let x3 = operands[n - 2];
                    let y3 = operands[n - 1];
                    let x2 = operands[n - 4];
                    let y2 = operands[n - 3];
                    let x1 = operands[n - 6];
                    let y1 = operands[n - 5];
                    surface
                        .path
                        .push(PathSegment::Curve(x1, y1, x2, y2, x3, y3));
                    surface.current_point = (x3, y3);
                }
            }
            "re" => {
                if operands.len() >= 4 {
                    let n = operands.len();
                    let x = operands[n - 4];
                    let y = operands[n - 3];
                    let w = operands[n - 2];
                    let h = operands[n - 1];
                    surface.path.push(PathSegment::Move(x, y));
                    surface.path.push(PathSegment::Line(x + w, y));
                    surface.path.push(PathSegment::Line(x + w, y + h));
                    surface.path.push(PathSegment::Line(x, y + h));
                    surface.path.push(PathSegment::Close);
                    surface.subpath_start = (x, y);
                    surface.current_point = (x, y);
                }
            }
            "h" => {
                surface.path.push(PathSegment::Close);
                surface.current_point = surface.subpath_start;
            }
            "S" | "s" => {
                surface.stroke_path();
                surface.path.clear();
            }
            "f" | "F" | "B" | "b" | "n" => {
                if op != "n" {
                    surface.fill_path();
                }
                if op == "B" || op == "b" {
                    surface.stroke_path();
                }
                surface.path.clear();
            }
            "BT" => {
                surface.in_text = true;
                surface.text_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                surface.text_line_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
            }
            "ET" => {
                surface.in_text = false;
            }
            "Tf" => {
                // Only the size lands in `operands` (the /Name token is not
                // numeric); the name is recovered from the token stream.
                if let Some(&size) = operands.last()
                    && size > 0.0
                {
                    surface.font_size = size;
                }
                if i >= 2
                    && let Some(name) = extract_font_name(&tokens, i)
                {
                    surface.font_metrics = fonts.get(&name).cloned();
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
                    surface.text_matrix = n;
                    surface.text_line_matrix = n;
                }
            }
            "Td" => {
                if operands.len() == 2 {
                    let (tx, ty) = (operands[0], operands[1]);
                    let m = surface.text_line_matrix;
                    surface.text_line_matrix = [
                        m[0],
                        m[1],
                        m[2],
                        m[3],
                        m[0] * tx + m[2] * ty + m[4],
                        m[1] * tx + m[3] * ty + m[5],
                    ];
                    surface.text_matrix = surface.text_line_matrix;
                }
            }
            "TD" => {
                if operands.len() == 2 {
                    let (tx, ty) = (operands[0], operands[1]);
                    let m = surface.text_line_matrix;
                    surface.text_line_matrix = [
                        m[0],
                        m[1],
                        m[2],
                        m[3],
                        m[0] * tx + m[2] * ty + m[4],
                        m[1] * tx + m[3] * ty + m[5],
                    ];
                    surface.text_matrix = surface.text_line_matrix;
                }
            }
            "T*" => {
                let m = surface.text_line_matrix;
                let new_ey = m[5] - surface.font_size;
                surface.text_line_matrix = [m[0], m[1], m[2], m[3], m[4], new_ey];
                surface.text_matrix = surface.text_line_matrix;
            }
            "Tj" => {
                if let Some(raw) = extract_string_raw(&tokens, i) {
                    draw_text_bytes(surface, &raw);
                }
            }
            "TJ" => {
                if let Some(raw) = extract_array_strings_raw(&tokens, i) {
                    draw_text_bytes(surface, &raw);
                }
            }
            "'" => {
                if let Some(raw) = extract_string_raw(&tokens, i) {
                    let m = surface.text_line_matrix;
                    let new_ey = m[5] - surface.font_size;
                    surface.text_line_matrix = [m[0], m[1], m[2], m[3], m[4], new_ey];
                    surface.text_matrix = surface.text_line_matrix;
                    draw_text_bytes(surface, &raw);
                }
            }
            "\"" => {
                // aw ac string
                if let Some(raw) = extract_string_raw(&tokens, i) {
                    draw_text_bytes(surface, &raw);
                }
            }
            _ => {
                // Unknown operator — skip
            }
        }
        operands.clear();
        i += 1;
    }
}

fn extract_font_name(tokens: &[String], i: usize) -> Option<String> {
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
fn extract_string_raw(tokens: &[String], i: usize) -> Option<Vec<u8>> {
    if i == 0 {
        return None;
    }
    let prev = &tokens[i - 1];
    let trimmed = prev.trim();
    if trimmed.starts_with('<') && trimmed.ends_with('>') {
        // Hex string
        let hex: String = trimmed[1..trimmed.len() - 1].chars().filter(|c| !c.is_whitespace()).collect();
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
                    b'n' => { out.push(b'\n'); k += 2; continue; }
                    b'r' => { out.push(b'\r'); k += 2; continue; }
                    b't' => { out.push(b'\t'); k += 2; continue; }
                    b'\\' | b'(' | b')' => { out.push(nxt); k += 2; continue; }
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
                    _ => { out.push(b); k += 1; continue; }
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
fn extract_array_strings_raw(tokens: &[String], i: usize) -> Option<Vec<u8>> {
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
                if cc == '(' { d += 1; } else if cc == ')' { d -= 1; }
                if d > 0 { end += 1; }
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
            let hex: String = prev[start..end].chars().filter(|ch| !ch.is_whitespace()).collect();
            for j in (0..hex.len()).step_by(2) {
                if j + 1 < hex.len() {
                    if let Ok(v) = u8::from_str_radix(&hex[j..j + 2], 16) {
                        out.push(v);
                    }
                } else if j < hex.len()
                    && let Ok(v) = u8::from_str_radix(&hex[j..j + 1], 16) {
                        out.push(v);
                    }
            }
            k = end + 1;
        } else {
            k += 1;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
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

/// Render text from raw content-stream bytes. For CIDFont (Type0) with embedded
/// TTF, bytes are interpreted as 2-byte glyph IDs. For simple fonts, bytes are
/// interpreted as ASCII/Latin-1 characters.
fn draw_text_bytes(surface: &mut Surface, raw: &[u8]) {
    if raw.is_empty() {
        return;
    }
    let size = surface.font_size;
    let color = surface.state.fill;
    let metrics = surface.font_metrics.clone();

    // If we have an embedded TTF, rasterise actual glyph outlines.
    if let Some(ttf_bytes) = metrics.as_ref().and_then(|m| m.embedded_ttf.as_ref())
        && let Ok(face) = ttf_parser::Face::parse(ttf_bytes, 0) {
            let upem = face.units_per_em().max(1) as f32;
            let scale = size / upem;
            let tm = surface.text_matrix;
            // Text-space cursor: baseline origin is (0,0); the text matrix maps
            // text space to user space, and transform_pt maps user → screen.
            let mut text_cursor: f32 = 0.0;

            // Determine if this is a 2-byte CIDFont (heuristic: even-length bytes
            // and font metrics has /W array with values > 255, or /Subtype /Type0).
            // For CIDFont, interpret bytes as 2-byte glyph IDs.
            // For simple fonts with embedded TTF, interpret as Unicode chars.
            let is_cid = raw.len().is_multiple_of(2) && raw.iter().filter(|&&b| b == 0).count() >= raw.len() / 4;

            if is_cid {
                // 2-byte glyph IDs
                let mut idx = 0;
                while idx + 1 < raw.len() {
                    let gid = u16::from_be_bytes([raw[idx], raw[idx + 1]]);
                    let gid_u32 = gid as u32;
                    let advance_units = metrics.as_ref().map(|m| m.advance(gid_u32)).unwrap_or(500);
                    let advance_pt = advance_units as f32 * size / 1000.0;

                    if gid > 0 {
                        let mut builder = GlyphOutlineBuilder::new();
                        let _ = face.outline_glyph(ttf_parser::GlyphId(gid), &mut builder);
                        for contour in &builder.contours {
                            if contour.len() < 3 {
                                continue;
                            }
                            let screen_poly: Vec<(f32, f32)> = contour
                                .iter()
                                .map(|(gx, gy)| {
                                    let px = text_cursor + gx * scale;
                                    let py = gy * scale;
                                    let tx = tm[0] * px + tm[2] * py + tm[4];
                                    let ty = tm[1] * px + tm[3] * py + tm[5];
                                    surface.transform_pt(tx, ty)
                                })
                                .collect();
                            fill_polygon_raw(surface, &screen_poly, color);
                        }
                    }
                    text_cursor += advance_pt;
                    idx += 2;
                }
            } else {
                // Simple font: interpret bytes as characters
                for &b in raw {
                    let ch = b as char;
                    let cp = ch as u32;
                    let advance_units = metrics.as_ref().map(|m| m.advance(cp)).unwrap_or(500);
                    let advance_pt = advance_units as f32 * size / 1000.0;

                    if let Some(gid) = face.glyph_index(ch) {
                        let mut builder = GlyphOutlineBuilder::new();
                        let _ = face.outline_glyph(gid, &mut builder);
                        for contour in &builder.contours {
                            if contour.len() < 3 {
                                continue;
                            }
                            let screen_poly: Vec<(f32, f32)> = contour
                                .iter()
                                .map(|(gx, gy)| {
                                    let px = text_cursor + gx * scale;
                                    let py = gy * scale;
                                    let tx = tm[0] * px + tm[2] * py + tm[4];
                                    let ty = tm[1] * px + tm[3] * py + tm[5];
                                    surface.transform_pt(tx, ty)
                                })
                                .collect();
                            fill_polygon_raw(surface, &screen_poly, color);
                        }
                    } else {
                        let ux = tm[0] * text_cursor + tm[4];
                        let uy = tm[1] * text_cursor + tm[5];
                        draw_glyph_rect(surface, ux, uy, advance_pt, size, color);
                    }
                    text_cursor += advance_pt;
                }
            }

            let m = surface.text_matrix;
            surface.text_matrix = [m[0], m[1], m[2], m[3], m[4] + text_cursor, m[5]];
            surface.text_line_matrix = surface.text_matrix;
            return;
        }

    // Fallback: gray glyph-block rectangles (base-14 or no embedded font).
    // Interpret bytes as characters for width lookup.
    let mut cursor_x = surface.text_matrix[4];
    let baseline_y = surface.text_matrix[5];
    for &b in raw {
        let cp = b as u32;
        let advance_units = metrics.as_ref().map(|m| m.advance(cp)).unwrap_or(500);
        let advance_pt = advance_units as f32 * size / 1000.0;
        draw_glyph_rect(surface, cursor_x, baseline_y, advance_pt, size, color);
        cursor_x += advance_pt;
    }

    let total = cursor_x - surface.text_matrix[4];
    let m = surface.text_matrix;
    surface.text_matrix = [m[0], m[1], m[2], m[3], m[4] + total, m[5]];
    surface.text_line_matrix = surface.text_matrix;
}

/// Draw a gray fallback rectangle for a glyph cell.
fn draw_glyph_rect(surface: &mut Surface, cursor_x: f32, baseline_y: f32, advance_pt: f32, size: f32, color: Color) {
    let cap_height = size * 0.7;
    let x0 = cursor_x;
    let y0 = baseline_y - cap_height * 0.2;
    let x1 = cursor_x + advance_pt * 0.92;
    let y1 = baseline_y + cap_height * 0.2;
    let (sx0, sy0) = surface.transform_pt(x0, y0);
    let (sx1, sy1) = surface.transform_pt(x1, y1);
    let min_x = sx0.min(sx1).round() as i32;
    let max_x = sx0.max(sx1).round() as i32;
    let min_y = sy0.min(sy1).round() as i32;
    let max_y = sy0.max(sy1).round() as i32;
    for py in min_y..=max_y {
        for px in min_x..=max_x {
            if px >= 0 && py >= 0 {
                surface.set_pixel(px as u32, py as u32, color, 0.55);
            }
        }
    }
}

/// Fill a polygon given in screen coordinates directly (no CTM applied).
fn fill_polygon_raw(surface: &mut Surface, poly: &[(f32, f32)], color: Color) {
    if poly.len() < 3 {
        return;
    }
    let mut min_y = poly[0].1;
    let mut max_y = poly[0].1;
    for p in poly {
        if p.1 < min_y {
            min_y = p.1;
        }
        if p.1 > max_y {
            max_y = p.1;
        }
    }
    let y_start = (min_y.floor() as i32).max(0);
    let y_end = (max_y.ceil() as i32).min(surface.height as i32 - 1);
    for y in y_start..=y_end {
        let yf = y as f32 + 0.5;
        let mut xs: Vec<f32> = Vec::new();
        let n = poly.len();
        for i in 0..n {
            let (x1, y1) = poly[i];
            let (x2, y2) = poly[(i + 1) % n];
            if (y1 <= yf && y2 > yf) || (y2 <= yf && y1 > yf) {
                let t = (yf - y1) / (y2 - y1);
                xs.push(x1 + (x2 - x1) * t);
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut i = 0;
        while i + 1 < xs.len() {
            let xa = xs[i];
            let xb = xs[i + 1];
            let x_start = (xa.ceil() as i32).max(0);
            let x_end = (xb.floor() as i32).min(surface.width as i32 - 1);
            if x_end >= x_start {
                for x in x_start..=x_end {
                    surface.set_pixel(x as u32, y as u32, color, 1.0);
                }
            }
            i += 2;
        }
    }
}

/// Builder that collects glyph outline contours from `ttf_parser`.
struct GlyphOutlineBuilder {
    contours: Vec<Vec<(f32, f32)>>,
    current: Vec<(f32, f32)>,
    start: (f32, f32),
    last: (f32, f32),
}

impl GlyphOutlineBuilder {
    fn new() -> Self {
        GlyphOutlineBuilder {
            contours: Vec::new(),
            current: Vec::new(),
            start: (0.0, 0.0),
            last: (0.0, 0.0),
        }
    }
}

impl ttf_parser::OutlineBuilder for GlyphOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        if !self.current.is_empty() {
            self.contours.push(std::mem::take(&mut self.current));
        }
        self.start = (x, y);
        self.last = (x, y);
        self.current.push((x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.current.push((x, y));
        self.last = (x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        // Flatten quadratic Bézier via midpoint subdivision.
        flatten_quad(&mut self.current, self.last, (x1, y1), (x, y));
        self.last = (x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        flatten_cubic_points(&mut self.current, self.last, (x1, y1), (x2, y2), (x, y));
        self.last = (x, y);
    }

    fn close(&mut self) {
        if !self.current.is_empty() {
            self.contours.push(std::mem::take(&mut self.current));
        }
        self.last = self.start;
    }
}

/// Flatten a quadratic Bézier into line segments using midpoint subdivision.
fn flatten_quad(out: &mut Vec<(f32, f32)>, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32)) {
    let mut stack = vec![(p0, p1, p2)];
    while let Some((a, b, c)) = stack.pop() {
        let chord = (c.0 - a.0).hypot(c.1 - a.1);
        let poly = (b.0 - a.0).hypot(b.1 - a.1) + (c.0 - b.0).hypot(c.1 - b.1);
        if chord < 0.5 || poly - chord < 0.5 {
            out.push(c);
        } else {
            let m01 = midpoint(a, b);
            let m12 = midpoint(b, c);
            let m012 = midpoint(m01, m12);
            stack.push((a, m01, m012));
            stack.push((m012, m12, c));
        }
    }
}

fn parse_number(s: &str) -> Option<f32> {
    s.parse::<f32>().ok()
}

fn cmyk_to_rgb(c: f32, m: f32, y: f32, k: f32) -> Color {
    let r = (1.0 - c) * (1.0 - k);
    let g = (1.0 - m) * (1.0 - k);
    let b = (1.0 - y) * (1.0 - k);
    Color::from_rgb(r, g, b)
}

// ----- Tokenizer ----------------------------------------------------------

fn tokenize(src: &str) -> Vec<String> {
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

// ----- Base 14 standard widths --------------------------------------------

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
        embedded_ttf: None,
    }
}

fn helvetica_widths() -> HashMap<u32, u16> {
    // PDF 32000-1:2008 Table H.3 (subset for typical ASCII + Latin-1 range)
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
        (91, 278),
        (92, 278),
        (93, 278),
        (94, 584),
        (95, 556),
        (96, 278),
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
        (123, 334),
        (124, 260),
        (125, 334),
        (126, 584),
        (127, 350),
    ];
    raw.iter().copied().collect()
}

fn times_widths() -> HashMap<u32, u16> {
    // PDF 32000-1:2008 Table H.5 (subset)
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
        (91, 333),
        (92, 278),
        (93, 333),
        (94, 469),
        (95, 500),
        (96, 333),
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
        (123, 480),
        (124, 200),
        (125, 480),
        (126, 541),
        (127, 350),
    ];
    raw.iter().copied().collect()
}

fn courier_widths() -> HashMap<u32, u16> {
    // Courier is monospaced: every char 600 units.
    let mut out = HashMap::new();
    for c in 32..=127 {
        out.insert(c, 600);
    }
    out
}

// ----- PNG encoder --------------------------------------------------------

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    if rgba.len() < (width as usize) * (height as usize) * 4 {
        return Err(anyhow!("pixel buffer too small for dimensions"));
    }
    let mut out = Vec::with_capacity(rgba.len() / 4 + rgba.len() / 8);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    // IHDR
    write_chunk(&mut out, b"IHDR", &ihdr_data(width, height));

    // IDAT — raw scanlines with filter byte 0 each.
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    let stride = width as usize * 4;
    for y in 0..height as usize {
        raw.push(0u8); // filter: None
        raw.extend_from_slice(&rgba[y * stride..y * stride + stride]);
    }
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&raw)?;
    let compressed = enc.finish()?;
    write_chunk(&mut out, b"IDAT", &compressed);

    // IEND
    write_chunk(&mut out, b"IEND", &[]);

    Ok(out)
}

fn ihdr_data(width: u32, height: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity(13);
    v.extend_from_slice(&width.to_be_bytes());
    v.extend_from_slice(&height.to_be_bytes());
    v.push(8); // bit depth
    v.push(6); // color type: RGBA
    v.push(0); // compression
    v.push(0); // filter
    v.push(0); // interlace
    v
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finalize().to_be_bytes());
}

/// Minimal CRC-32 (PNG polynomial).
struct Crc(u32);

const CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

impl Crc {
    fn new() -> Self {
        Crc(0xFFFF_FFFF)
    }
    fn update(&mut self, data: &[u8]) {
        for &b in data {
            let idx = ((self.0 ^ b as u32) & 0xFF) as usize;
            self.0 = CRC_TABLE[idx] ^ (self.0 >> 8);
        }
    }
    fn finalize(&self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements;
    use crate::pdf_generator::{PageLayout, generate_pdf_bytes};
    use crate::vector::{VectorCanvas, demo_canvas};

    #[test]
    fn rasterize_generated_text_pdf_yields_png() {
        let elements = elements::parse_markdown("# Hello\n\nBody text *with* `code`.");
        let pdf = generate_pdf_bytes(&elements, "Helvetica", 12.0, PageLayout::portrait()).unwrap();
        let page = rasterize_page(&pdf, 0, 72).unwrap();
        let png = page.to_png().unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert!(png.len() > 100);
    }

    #[test]
    fn rasterize_vector_demo_pdf_yields_png() {
        let pdf = demo_canvas().to_pdf_bytes(PageLayout::portrait()).unwrap();
        let page = rasterize_page(&pdf, 0, 96).unwrap();
        let png = page.to_png().unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]));
    }

    #[test]
    fn rasterize_all_returns_one_per_page() {
        let mut canvas = VectorCanvas::new();
        for _ in 0..3 {
            canvas = canvas.line(
                72.0,
                700.0,
                540.0,
                700.0,
                crate::pdf_generator::Color::black(),
                1.0,
            );
        }
        // Three pages via a multi-page helper (3 separate one-page PDFs merged
        // here isn't needed; we just verify a one-page document yields one raster).
        let pdf = canvas.to_pdf_bytes(PageLayout::portrait()).unwrap();
        let pages = rasterize_all(&pdf, 72).unwrap();
        assert_eq!(pages.len(), 1);
    }

    #[test]
    fn rasterize_landscape_wider_than_tall() {
        let pdf = demo_canvas().to_pdf_bytes(PageLayout::landscape()).unwrap();
        let page = rasterize_page(&pdf, 0, 72).unwrap();
        assert!(page.width > page.height);
    }

    #[test]
    fn rasterize_page_out_of_range_errors() {
        let pdf = demo_canvas().to_pdf_bytes(PageLayout::portrait()).unwrap();
        let err = rasterize_page(&pdf, 9, 72).expect_err("should fail");
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn raster_dimensions_scale_with_dpi() {
        let pdf = demo_canvas().to_pdf_bytes(PageLayout::portrait()).unwrap();
        let p72 = rasterize_page(&pdf, 0, 72).unwrap();
        let p144 = rasterize_page(&pdf, 0, 144).unwrap();
        assert_eq!(p144.width, p72.width * 2);
        assert_eq!(p144.height, p72.height * 2);
    }

    #[test]
    fn png_signature_is_correct() {
        let pixels = vec![0u8; 4];
        let png = encode_png(1, 1, &pixels).unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn raster_round_trips_through_png_decode() {
        // PNG signature + IHDR + IDAT + IEND structure should be present.
        let pdf = demo_canvas().to_pdf_bytes(PageLayout::portrait()).unwrap();
        let page = rasterize_page(&pdf, 0, 72).unwrap();
        let png = page.to_png().unwrap();
        let s = String::from_utf8_lossy(&png);
        assert!(s.contains("IHDR"));
        assert!(s.contains("IDAT"));
        assert!(s.contains("IEND"));
    }

    #[test]
    fn rasterize_unicode_text_with_embedded_font() {
        // Generate a PDF with Unicode text (forces embedded TTF font).
        let elements = elements::parse_markdown("# Hello αβγ\n\nUnicode text: 中文");
        let pdf = generate_pdf_bytes(&elements, "Helvetica", 12.0, PageLayout::portrait()).unwrap();
        let page = rasterize_page(&pdf, 0, 150).unwrap();
        let png = page.to_png().unwrap();
        // PNG should be valid and non-trivial in size.
        assert!(png.len() > 500);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        // The page should have non-white pixels (text was rendered).
        let non_white = page.pixels.chunks(4).filter(|px| px[0] < 250 || px[1] < 250 || px[2] < 250).count();
        assert!(non_white > 100, "expected rendered text pixels, got {non_white}");
    }
}
