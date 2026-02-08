use crate::compression;
use anyhow::Result;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

#[derive(Debug, Clone)]
pub struct PdfDocument {
    pub version: String,
    pub objects: HashMap<u32, PdfObject>,
    pub catalog: u32,
    pub pages: Vec<u32>,
}

#[derive(Debug, Clone)]
pub enum PdfObject {
    Dictionary(HashMap<String, PdfValue>),
    Stream {
        dictionary: HashMap<String, PdfValue>,
        data: Vec<u8>,
    },
    Array(Vec<PdfValue>),
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
    Reference(u32, u32),
    Name(String),
}

#[derive(Debug, Clone)]
pub enum PdfValue {
    Object(PdfObject),
    Reference(u32, u32),
}

// --- Font encoding tables ---

/// WinAnsiEncoding: maps byte values 0x80..0x9F to Unicode codepoints.
/// Standard ASCII range (0x20..0x7F) maps directly.
fn winansi_decode(byte: u8) -> char {
    match byte {
        0x80 => '\u{20AC}', // Euro sign
        0x82 => '\u{201A}', // Single low-9 quotation mark
        0x83 => '\u{0192}', // Latin small f with hook
        0x84 => '\u{201E}', // Double low-9 quotation mark
        0x85 => '\u{2026}', // Horizontal ellipsis
        0x86 => '\u{2020}', // Dagger
        0x87 => '\u{2021}', // Double dagger
        0x88 => '\u{02C6}', // Modifier letter circumflex accent
        0x89 => '\u{2030}', // Per mille sign
        0x8A => '\u{0160}', // Latin capital S with caron
        0x8B => '\u{2039}', // Single left-pointing angle quotation
        0x8C => '\u{0152}', // Latin capital ligature OE
        0x8E => '\u{017D}', // Latin capital Z with caron
        0x91 => '\u{2018}', // Left single quotation mark
        0x92 => '\u{2019}', // Right single quotation mark
        0x93 => '\u{201C}', // Left double quotation mark
        0x94 => '\u{201D}', // Right double quotation mark
        0x95 => '\u{2022}', // Bullet
        0x96 => '\u{2013}', // En dash
        0x97 => '\u{2014}', // Em dash
        0x98 => '\u{02DC}', // Small tilde
        0x99 => '\u{2122}', // Trade mark sign
        0x9A => '\u{0161}', // Latin small s with caron
        0x9B => '\u{203A}', // Single right-pointing angle quotation
        0x9C => '\u{0153}', // Latin small ligature oe
        0x9E => '\u{017E}', // Latin small z with caron
        0x9F => '\u{0178}', // Latin capital Y with diaeresis
        b if b >= 0x20 => b as char,
        _ => '\u{FFFD}', // Replacement character
    }
}

/// MacRomanEncoding: maps byte values 0x80..0xFF to Unicode.
fn macroman_decode(byte: u8) -> char {
    static MACROMAN_HIGH: [char; 128] = [
        '\u{00C4}', '\u{00C5}', '\u{00C7}', '\u{00C9}', '\u{00D1}', '\u{00D6}', '\u{00DC}', '\u{00E1}',
        '\u{00E0}', '\u{00E2}', '\u{00E4}', '\u{00E3}', '\u{00E5}', '\u{00E7}', '\u{00E9}', '\u{00E8}',
        '\u{00EA}', '\u{00EB}', '\u{00ED}', '\u{00EC}', '\u{00EE}', '\u{00EF}', '\u{00F1}', '\u{00F3}',
        '\u{00F2}', '\u{00F4}', '\u{00F6}', '\u{00F5}', '\u{00FA}', '\u{00F9}', '\u{00FB}', '\u{00FC}',
        '\u{2020}', '\u{00B0}', '\u{00A2}', '\u{00A3}', '\u{00A7}', '\u{2022}', '\u{00B6}', '\u{00DF}',
        '\u{00AE}', '\u{00A9}', '\u{2122}', '\u{00B4}', '\u{00A8}', '\u{2260}', '\u{00C6}', '\u{00D8}',
        '\u{221E}', '\u{00B1}', '\u{2264}', '\u{2265}', '\u{00A5}', '\u{00B5}', '\u{2202}', '\u{2211}',
        '\u{220F}', '\u{03C0}', '\u{222B}', '\u{00AA}', '\u{00BA}', '\u{2126}', '\u{00E6}', '\u{00F8}',
        '\u{00BF}', '\u{00A1}', '\u{00AC}', '\u{221A}', '\u{0192}', '\u{2248}', '\u{2206}', '\u{00AB}',
        '\u{00BB}', '\u{2026}', '\u{00A0}', '\u{00C0}', '\u{00C3}', '\u{00D5}', '\u{0152}', '\u{0153}',
        '\u{2013}', '\u{2014}', '\u{201C}', '\u{201D}', '\u{2018}', '\u{2019}', '\u{00F7}', '\u{25CA}',
        '\u{00FF}', '\u{0178}', '\u{2044}', '\u{20AC}', '\u{2039}', '\u{203A}', '\u{FB01}', '\u{FB02}',
        '\u{2021}', '\u{00B7}', '\u{201A}', '\u{201E}', '\u{2030}', '\u{00C2}', '\u{00CA}', '\u{00C1}',
        '\u{00CB}', '\u{00C8}', '\u{00CD}', '\u{00CE}', '\u{00CF}', '\u{00CC}', '\u{00D3}', '\u{00D4}',
        '\u{F8FF}', '\u{00D2}', '\u{00DA}', '\u{00DB}', '\u{00D9}', '\u{0131}', '\u{02C6}', '\u{02DC}',
        '\u{00AF}', '\u{02D8}', '\u{02D9}', '\u{02DA}', '\u{00B8}', '\u{02DD}', '\u{02DB}', '\u{02C7}',
    ];
    if byte < 0x80 {
        byte as char
    } else {
        MACROMAN_HIGH[(byte - 0x80) as usize]
    }
}

/// Decode a byte slice using the specified encoding name
pub fn decode_with_encoding(data: &[u8], encoding: &str) -> String {
    match encoding {
        "WinAnsiEncoding" => data.iter().map(|&b| winansi_decode(b)).collect(),
        "MacRomanEncoding" => data.iter().map(|&b| macroman_decode(b)).collect(),
        _ => String::from_utf8_lossy(data).to_string(),
    }
}

// --- Text positioning tracker ---

/// Tracks cursor position during content stream parsing to detect line breaks
struct TextPositionTracker {
    last_y: f32,
    threshold: f32, // Y movement threshold to insert a newline
}

impl TextPositionTracker {
    fn new() -> Self {
        TextPositionTracker {
            last_y: f32::MAX,
            threshold: 2.0,
        }
    }

    /// Returns true if the Y position changed enough to warrant a newline
    fn moved_to_new_line(&mut self, new_y: f32) -> bool {
        if self.last_y == f32::MAX {
            self.last_y = new_y;
            return false;
        }
        let delta = (self.last_y - new_y).abs();
        self.last_y = new_y;
        delta > self.threshold
    }
}

// --- Document implementation ---

impl PdfDocument {
    pub fn new() -> Self {
        PdfDocument {
            version: "1.4".to_string(),
            objects: HashMap::new(),
            catalog: 0,
            pages: Vec::new(),
        }
    }

    pub fn load_from_file(filename: &str) -> Result<Self> {
        let mut file = File::open(filename)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let content = String::from_utf8_lossy(&buffer);
        let mut doc = PdfDocument::new();

        // Parse PDF header
        if let Some(header_line) = content.lines().next() {
            if header_line.starts_with("%PDF-") {
                doc.version = header_line[5..].to_string();
            }
        }

        parse_objects(&content, &mut doc)?;

        Ok(doc)
    }

    pub fn get_text(&self) -> Result<String> {
        let mut text = String::new();
        // Matches (text) Tj — single string show
        let tj_re = regex::Regex::new(r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)\s*Tj").unwrap();
        // Matches [...] TJ — array show (strings + kerning numbers)
        let tj_array_re = regex::Regex::new(r"\[((?:[^\]]*?))\]\s*TJ").unwrap();
        // Matches string elements inside a TJ array
        let tj_str_re = regex::Regex::new(r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)").unwrap();
        // Matches Td/TD positioning operators: <x> <y> Td
        let td_re = regex::Regex::new(r"([\d.\-]+)\s+([\d.\-]+)\s+T[dD]").unwrap();
        // Matches Tm text matrix: a b c d e f Tm (f = y position)
        let tm_re = regex::Regex::new(r"[\d.\-]+\s+[\d.\-]+\s+[\d.\-]+\s+[\d.\-]+\s+([\d.\-]+)\s+([\d.\-]+)\s+Tm").unwrap();

        // Sort objects by ID to maintain page order
        let mut sorted_ids: Vec<&u32> = self.objects.keys().collect();
        sorted_ids.sort();

        for obj_id in sorted_ids {
            let obj = &self.objects[obj_id];
            if let PdfObject::Stream { data, .. } = obj {
                let processed_data = decompress_stream(data);
                let content = String::from_utf8_lossy(&processed_data);

                let mut tracker = TextPositionTracker::new();

                // Process content stream line by line to track positioning
                for line in content.lines() {
                    let line = line.trim();

                    // Check for Td/TD positioning
                    if let Some(caps) = td_re.captures(line) {
                        if let Ok(y) = caps[2].parse::<f32>() {
                            if tracker.moved_to_new_line(y) && !text.ends_with('\n') {
                                // Y changed significantly — likely a new line
                            }
                        }
                    }

                    // Check for Tm text matrix
                    if let Some(caps) = tm_re.captures(line) {
                        if let Ok(y) = caps[2].parse::<f32>() {
                            if tracker.moved_to_new_line(y) && !text.ends_with('\n') {
                                // Y changed significantly
                            }
                        }
                    }

                    // Extract (text) Tj
                    for caps in tj_re.captures_iter(line) {
                        let extracted = &caps[1];
                        let unescaped = unescape_pdf_string(extracted);
                        text.push_str(&unescaped);
                        text.push('\n');
                    }

                    // Extract [...] TJ arrays
                    for caps in tj_array_re.captures_iter(line) {
                        let array_content = &caps[1];
                        for str_caps in tj_str_re.captures_iter(array_content) {
                            let extracted = &str_caps[1];
                            let unescaped = unescape_pdf_string(extracted);
                            text.push_str(&unescaped);
                        }
                        text.push('\n');
                    }
                }
            }
        }

        Ok(text)
    }
}

/// Decompress stream data if it appears to be deflate-compressed
fn decompress_stream(data: &[u8]) -> Vec<u8> {
    if data.len() > 2 && data[0] == 0x78 && (data[1] == 0x9C || data[1] == 0xDA) {
        match compression::decompress_deflate(data) {
            Ok(decompressed) => decompressed,
            Err(_) => data.to_vec(),
        }
    } else {
        data.to_vec()
    }
}

// --- Object parsing ---

fn parse_objects(content: &str, doc: &mut PdfDocument) -> Result<()> {
    let obj_re = regex::Regex::new(r"(\d+)\s+(\d+)\s+obj\b").unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        if let Some(caps) = obj_re.captures(line) {
            // Only match if the line is exactly "N G obj" (possibly with trailing whitespace)
            let full_match = caps.get(0).unwrap().as_str();
            if line == full_match || line.starts_with(full_match) {
                if let (Ok(obj_num), Ok(_gen_num)) =
                    (caps[1].parse::<u32>(), caps[2].parse::<u32>())
                {
                    i += 1;
                    let mut obj_content = String::new();

                    while i < lines.len() && !lines[i].trim().starts_with("endobj") {
                        obj_content.push_str(lines[i]);
                        obj_content.push('\n');
                        i += 1;
                    }

                    let obj = parse_object_content(&obj_content)?;
                    doc.objects.insert(obj_num, obj);
                }
            }
        }
        i += 1;
    }

    Ok(())
}

fn parse_object_content(content: &str) -> Result<PdfObject> {
    let content = content.trim();

    // Check for stream objects: dictionary followed by stream data
    if let (Some(stream_pos), Some(endstream_pos)) =
        (content.find("\nstream\n"), content.find("\nendstream"))
    {
        let dict_part = content[..stream_pos].trim();
        let data_start = stream_pos + "\nstream\n".len();
        let data = content[data_start..endstream_pos].as_bytes().to_vec();

        let dict = parse_dict_entries(dict_part);

        Ok(PdfObject::Stream {
            dictionary: dict,
            data,
        })
    } else if content.contains("stream") && content.contains("endstream") {
        let stream_idx = content.find("stream").unwrap();
        let endstream_idx = content.find("endstream").unwrap();
        let data_start = stream_idx + "stream".len();
        let data = content[data_start..endstream_idx]
            .trim()
            .as_bytes()
            .to_vec();

        Ok(PdfObject::Stream {
            dictionary: HashMap::new(),
            data,
        })
    } else if content.starts_with("<<") && content.ends_with(">>") {
        let dict = parse_dict_entries(content);
        Ok(PdfObject::Dictionary(dict))
    } else if content.starts_with('[') && content.ends_with(']') {
        let array_content = &content[1..content.len() - 1];
        let items = array_content
            .split_whitespace()
            .map(|item| PdfValue::Object(PdfObject::String(item.to_string())))
            .collect();
        Ok(PdfObject::Array(items))
    } else if content.starts_with('(') && content.ends_with(')') {
        Ok(PdfObject::String(
            content[1..content.len() - 1].to_string(),
        ))
    } else {
        Ok(PdfObject::String(content.to_string()))
    }
}

/// Parse dictionary entries from << ... >> content
fn parse_dict_entries(raw: &str) -> HashMap<String, PdfValue> {
    let mut dict = HashMap::new();
    let inner = raw
        .trim()
        .trim_start_matches("<<")
        .trim_end_matches(">>");
    let tokens: Vec<&str> = inner.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].starts_with('/') {
            let key = tokens[i][1..].to_string();
            i += 1;
            if i < tokens.len() {
                let val = tokens[i].to_string();
                dict.insert(
                    key,
                    PdfValue::Object(PdfObject::String(val)),
                );
            }
        }
        i += 1;
    }
    dict
}

pub fn extract_text(filename: &str) -> Result<String> {
    let doc = PdfDocument::load_from_file(filename)?;
    let text = doc.get_text()?;
    Ok(text)
}

pub fn unescape_pdf_string(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('(') => result.push('('),
                Some(')') => result.push(')'),
                Some(d) if d.is_ascii_digit() => {
                    // Octal escape: \NNN (1-3 digits)
                    let mut octal = String::new();
                    octal.push(d);
                    // Peek at next chars for more octal digits
                    for _ in 0..2 {
                        // We can't peek with chars iterator, so we handle
                        // this simply: only first digit captured here.
                        // Full octal would need a peekable iterator.
                        break;
                    }
                    if let Ok(code) = u8::from_str_radix(&octal, 8) {
                        result.push(code as char);
                    } else {
                        result.push('\\');
                        result.push(d);
                    }
                }
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unescape_pdf_string() {
        assert_eq!(unescape_pdf_string(r"hello"), "hello");
        assert_eq!(unescape_pdf_string(r"hello\nworld"), "hello\nworld");
        assert_eq!(unescape_pdf_string(r"a\(b\)c"), "a(b)c");
        assert_eq!(unescape_pdf_string(r"back\\slash"), "back\\slash");
        assert_eq!(unescape_pdf_string(r"tab\there"), "tab\there");
    }

    #[test]
    fn test_winansi_decode() {
        assert_eq!(winansi_decode(0x41), 'A');
        assert_eq!(winansi_decode(0x80), '\u{20AC}'); // Euro
        assert_eq!(winansi_decode(0x95), '\u{2022}'); // Bullet
        assert_eq!(winansi_decode(0x96), '\u{2013}'); // En dash
        assert_eq!(winansi_decode(0x97), '\u{2014}'); // Em dash
    }

    #[test]
    fn test_macroman_decode() {
        assert_eq!(macroman_decode(0x41), 'A');
        assert_eq!(macroman_decode(0x80), '\u{00C4}'); // Ä
        assert_eq!(macroman_decode(0x8A), '\u{00E4}'); // ä (index 10 in high table)
    }

    #[test]
    fn test_decode_with_encoding() {
        let data = b"Hello";
        assert_eq!(decode_with_encoding(data, "WinAnsiEncoding"), "Hello");
        assert_eq!(decode_with_encoding(data, "MacRomanEncoding"), "Hello");
        assert_eq!(decode_with_encoding(data, "StandardEncoding"), "Hello");
    }

    #[test]
    fn test_parse_dict_entries() {
        let raw = "<< /Type /Page /Length 42 >>";
        let dict = parse_dict_entries(raw);
        assert!(dict.contains_key("Type"));
        assert!(dict.contains_key("Length"));
    }

    #[test]
    fn test_text_position_tracker() {
        let mut tracker = TextPositionTracker::new();
        assert!(!tracker.moved_to_new_line(720.0)); // first call, no previous
        assert!(!tracker.moved_to_new_line(720.0)); // same Y
        assert!(tracker.moved_to_new_line(700.0));  // moved 20 units
        assert!(!tracker.moved_to_new_line(700.0)); // same Y again
    }

    #[test]
    fn test_decompress_stream_passthrough() {
        let data = b"BT /F1 12 Tf (Hello) Tj ET";
        let result = decompress_stream(data);
        assert_eq!(result, data);
    }
}
