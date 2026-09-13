//! Text extraction: `PdfDocument::get_text`, ToUnicode CMap handling,
//! and glyph-ID reverse mapping for embedded Unicode fonts.

use super::decode::{
    TextPositionTracker, decode_pdf_hex_string_with_map, decode_utf16be, unescape_pdf_string,
};
use super::objects::{PdfDocument, PdfObject};
use super::parser::decompress_stream;
use super::{re_td, re_tj, re_tj_array, re_tj_hex, re_tj_hex_str, re_tj_str, re_tm};
use crate::error::Result;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

impl PdfDocument {
    pub fn get_text(&self) -> Result<String> {
        let mut text = String::new();
        let tounicode = collect_tounicode_gid_map(self);
        // Matches (text) Tj — single string show
        let tj_re = re_tj();
        // Matches <hex> Tj — hex string show
        let tj_hex_re = re_tj_hex();
        // Matches [...] TJ — array show (strings + kerning numbers)
        let tj_array_re = re_tj_array();
        // Matches string elements inside a TJ array
        let tj_str_re = re_tj_str();
        // Matches hex string elements inside a TJ array
        let tj_hex_str_re = re_tj_hex_str();
        // Matches Td/TD positioning operators: <x> <y> Td
        let td_re = re_td();
        // Matches Tm text matrix: a b c d e f Tm (f = y position)
        let tm_re = re_tm();

        // Sort objects by ID to maintain page order
        let mut sorted_ids: Vec<&u32> = self.objects.keys().collect();
        sorted_ids.sort();

        for obj_id in sorted_ids {
            let obj = &self.objects[obj_id];
            if let PdfObject::Stream { data, .. } = obj {
                let processed_data = decompress_stream(data);
                let content = String::from_utf8_lossy(&processed_data);

                let mut tracker = TextPositionTracker::new();
                // Only insert a word space when a positioning op occurred since the last
                // text show. Sequential `Tj` (e.g. syntax-highlighted spans) must stay contiguous.
                let mut space_before_next_text = false;

                // Process content stream line by line to track positioning
                for line in content.lines() {
                    let line = line.trim();

                    // Check for Td/TD positioning BEFORE extracting text on this line
                    if let Some(caps) = td_re.captures(line)
                        && let Ok(y) = caps[2].parse::<f32>()
                    {
                        if tracker.moved_to_new_line(y) && !text.ends_with('\n') {
                            text.push('\n');
                            space_before_next_text = false;
                        } else {
                            space_before_next_text = true;
                        }
                    }

                    // Check for Tm text matrix BEFORE extracting text on this line
                    if let Some(caps) = tm_re.captures(line)
                        && let Ok(y) = caps[2].parse::<f32>()
                    {
                        if tracker.moved_to_new_line(y) && !text.ends_with('\n') {
                            text.push('\n');
                            space_before_next_text = false;
                        } else if !text.is_empty() && !text.ends_with('\n') {
                            // Same-line Tm reposition (e.g. next word) → word gap
                            space_before_next_text = true;
                        }
                    }

                    // Extract (text) Tj
                    for caps in tj_re.captures_iter(line) {
                        let extracted = &caps[1];
                        let unescaped = unescape_pdf_string(extracted);
                        if space_before_next_text && !text.ends_with(' ') && !text.ends_with('\n') {
                            text.push(' ');
                        }
                        text.push_str(&unescaped);
                        space_before_next_text = false;
                    }

                    // Extract <hex> Tj
                    for caps in tj_hex_re.captures_iter(line) {
                        let hex_str = caps[1].replace(char::is_whitespace, "");
                        let decoded = decode_pdf_hex_string_with_map(&hex_str, Some(&tounicode));
                        if space_before_next_text && !text.ends_with(' ') && !text.ends_with('\n') {
                            text.push(' ');
                        }
                        text.push_str(&decoded);
                        space_before_next_text = false;
                    }

                    // Extract [...] TJ arrays — adjacent strings are contiguous (no auto spaces)
                    for caps in tj_array_re.captures_iter(line) {
                        let array_content = &caps[1];

                        for str_caps in tj_str_re.captures_iter(array_content) {
                            let extracted = &str_caps[1];
                            let unescaped = unescape_pdf_string(extracted);
                            if space_before_next_text
                                && !text.ends_with(' ')
                                && !text.ends_with('\n')
                            {
                                text.push(' ');
                            }
                            text.push_str(&unescaped);
                            space_before_next_text = false;
                        }

                        for hex_caps in tj_hex_str_re.captures_iter(array_content) {
                            let hex_str = hex_caps[1].replace(char::is_whitespace, "");
                            let decoded =
                                decode_pdf_hex_string_with_map(&hex_str, Some(&tounicode));
                            if space_before_next_text
                                && !text.ends_with(' ')
                                && !text.ends_with('\n')
                            {
                                text.push(' ');
                            }
                            text.push_str(&decoded);
                            space_before_next_text = false;
                        }
                    }
                }

                // Add newline at the end of each page's content
                if !text.ends_with('\n') && !text.is_empty() {
                    text.push('\n');
                }
            }
        }

        Ok(text)
    }
}

pub fn extract_text(filename: &str) -> Result<String> {
    let doc = PdfDocument::load_from_file(filename)?;
    let text = doc.get_text()?;
    Ok(text)
}

pub(crate) fn collect_tounicode_gid_map(doc: &PdfDocument) -> HashMap<u16, char> {
    let mut map = HashMap::new();
    static PAIR_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pair_re = PAIR_RE
        .get_or_init(|| regex::Regex::new(r"<([0-9A-Fa-f]{4})>\s*<([0-9A-Fa-f]+)>").unwrap());
    for obj in doc.objects.values() {
        let PdfObject::Stream { data, .. } = obj else {
            continue;
        };
        let processed = decompress_stream(data);
        let content = String::from_utf8_lossy(&processed);
        if !(content.contains("beginbfchar") || content.contains("begincmap")) {
            continue;
        }
        for caps in pair_re.captures_iter(&content) {
            let Ok(gid) = u16::from_str_radix(&caps[1], 16) else {
                continue;
            };
            let uni_hex = &caps[2];
            if uni_hex.len() < 4 || !uni_hex.len().is_multiple_of(4) {
                continue;
            }
            let mut units = Vec::new();
            for i in (0..uni_hex.len()).step_by(4) {
                if let Ok(unit) = u16::from_str_radix(&uni_hex[i..i + 4], 16) {
                    units.push(unit);
                }
            }
            let mut bytes = Vec::with_capacity(units.len() * 2);
            for u in units {
                bytes.push((u >> 8) as u8);
                bytes.push((u & 0xFF) as u8);
            }
            let decoded = decode_utf16be(&bytes);
            if let Some(ch) = decoded.chars().next() {
                map.insert(gid, ch);
            }
        }
    }
    map
}

pub(super) fn resolve_unicode_ttf_path_for_extraction() -> Option<String> {
    if let Ok(path) = std::env::var("PDFRS_UNICODE_FONT_PATH")
        && !path.trim().is_empty()
        && Path::new(&path).exists()
    {
        return Some(path);
    }

    let candidates = [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
    ];

    candidates
        .iter()
        .find(|p| Path::new(p).exists())
        .map(|p| (*p).to_string())
}

pub(super) fn build_unicode_gid_reverse_map() -> Option<HashMap<u16, char>> {
    let font_path = resolve_unicode_ttf_path_for_extraction()?;
    let font_bytes = fs::read(font_path).ok()?;
    let face = ttf_parser::Face::parse(&font_bytes, 0).ok()?;

    let mut reverse_map = HashMap::new();

    // Scan the BMP (U+0000–U+FFFF) instead of the full Unicode range
    // (U+0000–U+10FFFF). This covers all common text scripts and is 16× faster.
    for cp in 0x0001u32..=0xFFFF {
        let Some(ch) = char::from_u32(cp) else {
            continue;
        };
        if let Some(glyph) = face.glyph_index(ch) {
            reverse_map.entry(glyph.0).or_insert(ch);
        }
    }

    Some(reverse_map)
}

pub(super) fn decode_unicode_glyph_id_bytes_with_map(
    bytes: &[u8],
    tounicode: Option<&HashMap<u16, char>>,
) -> Option<String> {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return None;
    }

    if let Some(map) = tounicode
        && !map.is_empty()
    {
        let mut out = String::with_capacity(bytes.len() / 2);
        let mut known = 0usize;
        let total = bytes.len() / 2;
        for chunk in bytes.chunks_exact(2) {
            let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
            if let Some(ch) = map.get(&gid) {
                out.push(*ch);
                known += 1;
            } else if gid == 0 {
                out.push(' ');
            } else {
                out.push('\u{FFFD}');
            }
        }
        if known * 10 >= total * 6 {
            return Some(out);
        }
    }

    static GID_REVERSE_MAP: OnceLock<Option<HashMap<u16, char>>> = OnceLock::new();
    let gid_map = GID_REVERSE_MAP
        .get_or_init(build_unicode_gid_reverse_map)
        .as_ref()?;

    let mut out = String::with_capacity(bytes.len() / 2);
    let mut known_count = 0usize;
    let total = bytes.len() / 2;

    for chunk in bytes.chunks_exact(2) {
        let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
        if let Some(ch) = gid_map.get(&gid) {
            out.push(*ch);
            known_count += 1;
        } else if gid == 0 {
            out.push(' ');
        } else {
            out.push('\u{FFFD}');
        }
    }

    // Require a strong hit-rate to avoid mis-decoding arbitrary hex payloads.
    if known_count == 0 || known_count * 10 < total * 6 {
        return None;
    }

    Some(out)
}
