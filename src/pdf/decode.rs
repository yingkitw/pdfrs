//! Text decoding helpers: WinAnsi/MacRoman encodings, PDF string unescaping,
//! hex string decoding, UTF-16BE, and content-stream position tracking.

use std::collections::HashMap;

use super::text_extract::decode_unicode_glyph_id_bytes_with_map;

pub(super) fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// --- Font encoding tables ---

/// WinAnsiEncoding: maps byte values 0x80..0x9F to Unicode codepoints.
/// Standard ASCII range (0x20..0x7F) maps directly.
pub(super) fn winansi_decode(byte: u8) -> char {
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
pub(super) fn macroman_decode(byte: u8) -> char {
    static MACROMAN_HIGH: [char; 128] = [
        '\u{00C4}', '\u{00C5}', '\u{00C7}', '\u{00C9}', '\u{00D1}', '\u{00D6}', '\u{00DC}',
        '\u{00E1}', '\u{00E0}', '\u{00E2}', '\u{00E4}', '\u{00E3}', '\u{00E5}', '\u{00E7}',
        '\u{00E9}', '\u{00E8}', '\u{00EA}', '\u{00EB}', '\u{00ED}', '\u{00EC}', '\u{00EE}',
        '\u{00EF}', '\u{00F1}', '\u{00F3}', '\u{00F2}', '\u{00F4}', '\u{00F6}', '\u{00F5}',
        '\u{00FA}', '\u{00F9}', '\u{00FB}', '\u{00FC}', '\u{2020}', '\u{00B0}', '\u{00A2}',
        '\u{00A3}', '\u{00A7}', '\u{2022}', '\u{00B6}', '\u{00DF}', '\u{00AE}', '\u{00A9}',
        '\u{2122}', '\u{00B4}', '\u{00A8}', '\u{2260}', '\u{00C6}', '\u{00D8}', '\u{221E}',
        '\u{00B1}', '\u{2264}', '\u{2265}', '\u{00A5}', '\u{00B5}', '\u{2202}', '\u{2211}',
        '\u{220F}', '\u{03C0}', '\u{222B}', '\u{00AA}', '\u{00BA}', '\u{2126}', '\u{00E6}',
        '\u{00F8}', '\u{00BF}', '\u{00A1}', '\u{00AC}', '\u{221A}', '\u{0192}', '\u{2248}',
        '\u{2206}', '\u{00AB}', '\u{00BB}', '\u{2026}', '\u{00A0}', '\u{00C0}', '\u{00C3}',
        '\u{00D5}', '\u{0152}', '\u{0153}', '\u{2013}', '\u{2014}', '\u{201C}', '\u{201D}',
        '\u{2018}', '\u{2019}', '\u{00F7}', '\u{25CA}', '\u{00FF}', '\u{0178}', '\u{2044}',
        '\u{20AC}', '\u{2039}', '\u{203A}', '\u{FB01}', '\u{FB02}', '\u{2021}', '\u{00B7}',
        '\u{201A}', '\u{201E}', '\u{2030}', '\u{00C2}', '\u{00CA}', '\u{00C1}', '\u{00CB}',
        '\u{00C8}', '\u{00CD}', '\u{00CE}', '\u{00CF}', '\u{00CC}', '\u{00D3}', '\u{00D4}',
        '\u{F8FF}', '\u{00D2}', '\u{00DA}', '\u{00DB}', '\u{00D9}', '\u{0131}', '\u{02C6}',
        '\u{02DC}', '\u{00AF}', '\u{02D8}', '\u{02D9}', '\u{02DA}', '\u{00B8}', '\u{02DD}',
        '\u{02DB}', '\u{02C7}',
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
pub(super) struct TextPositionTracker {
    last_y: f32,
    threshold: f32, // Y movement threshold to insert a newline
}

impl TextPositionTracker {
    pub(super) fn new() -> Self {
        TextPositionTracker {
            last_y: f32::MAX,
            threshold: 2.0,
        }
    }

    /// Returns true if the Y position changed enough to warrant a newline
    pub(super) fn moved_to_new_line(&mut self, new_y: f32) -> bool {
        if self.last_y == f32::MAX {
            self.last_y = new_y;
            return false;
        }
        let delta = (self.last_y - new_y).abs();
        self.last_y = new_y;
        delta > self.threshold
    }
}

pub fn unescape_pdf_string(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('(') => result.push('('),
                Some(')') => result.push(')'),
                Some('b') => result.push('\u{0008}'),
                Some('f') => result.push('\u{000C}'),
                Some(d) if d.is_ascii_digit() => {
                    let mut octal = String::new();
                    octal.push(d);
                    for _ in 0..2 {
                        if let Some(&next) = chars.peek() {
                            if next.is_ascii_digit() && ('0'..='7').contains(&next) {
                                octal.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    if let Ok(code) = u8::from_str_radix(&octal, 8) {
                        if code > 0 {
                            result.push(code as char);
                        }
                    } else {
                        result.push('\\');
                        result.push(d);
                    }
                }
                Some(other) => {
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

pub fn decode_pdf_hex_string(s: &str) -> String {
    decode_pdf_hex_string_with_map(s, None)
}

pub(crate) fn decode_pdf_hex_string_with_map(
    s: &str,
    tounicode: Option<&HashMap<u16, char>>,
) -> String {
    let hex_str: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut bytes = Vec::new();

    for i in (0..hex_str.len()).step_by(2) {
        if i + 1 < hex_str.len() {
            let byte_str = &hex_str[i..i + 2];
            if let Ok(byte) = u8::from_str_radix(byte_str, 16) {
                bytes.push(byte);
            }
        } else if i < hex_str.len() {
            let byte_str = &hex_str[i..i + 1];
            if let Ok(byte) = u8::from_str_radix(&format!("{}0", byte_str), 16) {
                bytes.push(byte);
            }
        }
    }

    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        decode_utf16be(&bytes[2..])
    } else {
        if let Some(decoded) = decode_unicode_glyph_id_bytes_with_map(&bytes, tounicode) {
            return decoded;
        }
        String::from_utf8_lossy(&bytes).to_string()
    }
}

pub(super) fn decode_utf16be(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;

    while i + 1 < bytes.len() {
        let high = (bytes[i] as u16) << 8 | (bytes[i + 1] as u16);
        i += 2;

        if (0xD800..=0xDBFF).contains(&high) && i + 1 < bytes.len() {
            let low = (bytes[i] as u16) << 8 | (bytes[i + 1] as u16);
            if (0xDC00..=0xDFFF).contains(&low) {
                i += 2;
                let codepoint = 0x10000u32 + ((high as u32 - 0xD800) << 10) + (low as u32 - 0xDC00);
                if let Some(ch) = char::from_u32(codepoint) {
                    result.push(ch);
                }
                continue;
            }
        }

        if let Some(ch) = char::from_u32(high as u32) {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::text_extract::resolve_unicode_ttf_path_for_extraction;
    use std::fs;

    #[test]
    fn test_unescape_pdf_string() {
        assert_eq!(unescape_pdf_string(r"hello"), "hello");
        assert_eq!(unescape_pdf_string(r"hello\nworld"), "hello\nworld");
        assert_eq!(unescape_pdf_string(r"a\(b\)c"), "a(b)c");
        assert_eq!(unescape_pdf_string(r"back\\slash"), "back\\slash");
        assert_eq!(unescape_pdf_string(r"tab\there"), "tab\there");
        assert_eq!(unescape_pdf_string(r"form\ffeed"), "form\u{000C}feed");
        assert_eq!(unescape_pdf_string(r"back\bspace"), "back\u{0008}space");
    }

    #[test]
    fn test_unescape_octal_sequences() {
        assert_eq!(unescape_pdf_string(r"\101"), "A");
        assert_eq!(unescape_pdf_string(r"\101\102\103"), "ABC");
        assert_eq!(unescape_pdf_string(r"\60"), "0");
        assert_eq!(unescape_pdf_string(r"\141\142\143"), "abc");
        assert_eq!(unescape_pdf_string(r"Hello\40World"), "Hello World");
    }

    #[test]
    fn test_decode_hex_string_basic() {
        assert_eq!(decode_pdf_hex_string("48656C6C6F"), "Hello");
        assert_eq!(decode_pdf_hex_string("576F726C64"), "World");
        assert_eq!(decode_pdf_hex_string("414243"), "ABC");
        assert_eq!(decode_pdf_hex_string("48 65 6C 6C 6F"), "Hello");
    }

    #[test]
    fn test_decode_hex_string_utf16be() {
        assert_eq!(decode_pdf_hex_string("FEFF00480065006C006C006F"), "Hello");
        assert_eq!(decode_pdf_hex_string("FEFF4F60597D"), "你好");
        assert_eq!(decode_pdf_hex_string("FEFF0041004200430044"), "ABCD");
    }

    #[test]
    fn test_decode_hex_string_unicode_symbols() {
        assert_eq!(decode_pdf_hex_string("FEFF03B103B203B3"), "αβγ");
        assert_eq!(decode_pdf_hex_string("FEFF221E2211222B"), "∞∑∫");
    }

    #[test]
    fn test_decode_hex_string_unicode_glyph_ids_roundtrip() {
        let Some(path) = resolve_unicode_ttf_path_for_extraction() else {
            return;
        };
        let Ok(bytes) = fs::read(path) else {
            return;
        };
        let Ok(face) = ttf_parser::Face::parse(&bytes, 0) else {
            return;
        };

        let sample = "Unicode test: 你好 Γεια ∑";
        let mut encoded = String::new();
        for ch in sample.chars() {
            let Some(gid) = face.glyph_index(ch) else {
                return;
            };
            encoded.push_str(&format!("{:04X}", gid.0));
        }

        assert_eq!(decode_pdf_hex_string(&encoded), sample);
    }

    #[test]
    fn test_decode_utf16be_surrogate_pairs() {
        let bytes = vec![0xD8, 0x3D, 0xDE, 0x00];
        assert_eq!(decode_utf16be(&bytes), "😀");

        let bytes2 = vec![0xD8, 0x3D, 0xDE, 0x01];
        assert_eq!(decode_utf16be(&bytes2), "😁");
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
    fn test_text_position_tracker() {
        let mut tracker = TextPositionTracker::new();
        assert!(!tracker.moved_to_new_line(720.0)); // first call, no previous
        assert!(!tracker.moved_to_new_line(720.0)); // same Y
        assert!(tracker.moved_to_new_line(700.0)); // moved 20 units
        assert!(!tracker.moved_to_new_line(700.0)); // same Y again
    }
}
