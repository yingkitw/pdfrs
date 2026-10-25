//! Text helpers for PDF generation: math rendering, string escaping, Base-14 encoding.

mod encode;
mod math;

pub(crate) use encode::{encode_pdf_text, escape_pdf_string, use_base14_normalization};
pub(crate) use math::{flatten_math_environments, parse_brace_group, render_math_text};

#[cfg(test)]
mod tests {
    use super::encode::normalize_for_base14_font;
    use super::*;

    #[test]
    fn encode_pdf_text_preserves_ascii() {
        assert_eq!(encode_pdf_text("Hello"), "(Hello)");
    }

    #[test]
    fn encode_pdf_text_emits_soft_hyphen_as_utf16be() {
        // U+00AD is non-ASCII so the encoder must wrap the string in UTF-16BE
        // hex (with BOM) so the soft hyphen survives the round trip.
        let encoded = encode_pdf_text("co\u{00AD}operate");
        assert!(
            encoded.starts_with('<') && encoded.ends_with('>'),
            "expected hex-encoded string, got {encoded}"
        );
        let hex = encoded.trim_start_matches('<').trim_end_matches('>');
        // BOM + 'c' + 'o' + U+00AD + 'o' + 'p' + 'e' + 'r' + 'a' + 't' + 'e'
        let expected = "FEFF0063006F00AD006F0070006500720061007400".to_string() + "65";
        assert_eq!(hex, expected);
    }

    #[test]
    fn encode_pdf_text_round_trip_with_soft_hyphen() {
        let original = "hy\u{00AD}phen\u{00AD}ation";
        let encoded = encode_pdf_text(original);
        let hex = encoded.trim_start_matches('<').trim_end_matches('>');
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        // Strip the 2-byte BOM and decode UTF-16BE inline to verify the
        // emission is self-consistent without depending on a private decoder.
        let mut decoded = String::new();
        let mut i = 2;
        while i + 1 < bytes.len() {
            let unit = u16::from_be_bytes([bytes[i], bytes[i + 1]]);
            i += 2;
            if let Some(ch) = char::from_u32(unit as u32) {
                decoded.push(ch);
            }
        }
        assert_eq!(decoded, original);
    }

    #[test]
    fn base14_normalization_soft_hyphen_to_visible_hyphen() {
        // Exercise the fallback helper directly to avoid touching global env
        // state that would race with other tests sharing the same process.
        let normalized = normalize_for_base14_font("co\u{00AD}op");
        assert!(
            !normalized.contains('\u{00AD}'),
            "soft hyphen should not survive base-14 fallback: {normalized:?}"
        );
        assert!(normalized.contains('-'), "got: {normalized:?}");
    }
}
