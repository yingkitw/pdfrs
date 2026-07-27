//! Right-to-left (RTL) text helpers for PDF generation
//!
//! Provides script detection and a lean visual reorder for RTL-dominant
//! strings so they display correctly in left-to-right PDF text operators.
//! This is not a full Unicode Bidirectional Algorithm implementation.

/// Overall text direction of a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDirection {
    Ltr,
    Rtl,
    /// Mixed LTR/RTL content (visual reorder applied only to RTL runs)
    Mixed,
    Neutral,
}

/// True for characters in common RTL scripts (Hebrew, Arabic, Syriac, etc.).
pub fn is_rtl_char(c: char) -> bool {
    matches!(
        c,
        '\u{0590}'..='\u{05FF}' // Hebrew
            | '\u{0600}'..='\u{06FF}' // Arabic
            | '\u{0700}'..='\u{074F}' // Syriac
            | '\u{0750}'..='\u{077F}' // Arabic Supplement
            | '\u{08A0}'..='\u{08FF}' // Arabic Extended-A
            | '\u{FB1D}'..='\u{FDFF}' // Hebrew/Arabic presentation forms
            | '\u{FE70}'..='\u{FEFF}' // Arabic presentation forms-B
    )
}

/// True for strong LTR letters (ASCII Latin or other LTR letters).
pub fn is_ltr_char(c: char) -> bool {
    c.is_ascii_alphabetic()
        || matches!(
            c,
            '\u{00C0}'..='\u{024F}' // Latin-1 / Latin Extended
                | '\u{0400}'..='\u{04FF}' // Cyrillic
                | '\u{0370}'..='\u{03FF}' // Greek
                | '\u{4E00}'..='\u{9FFF}' // CJK (treated as LTR for layout)
        )
}

/// Detect overall direction from character counts.
pub fn detect_text_direction(text: &str) -> TextDirection {
    let mut rtl = 0usize;
    let mut ltr = 0usize;
    for c in text.chars() {
        if is_rtl_char(c) {
            rtl += 1;
        } else if is_ltr_char(c) {
            ltr += 1;
        }
    }
    match (rtl > 0, ltr > 0) {
        (true, false) => TextDirection::Rtl,
        (false, true) => TextDirection::Ltr,
        (true, true) => TextDirection::Mixed,
        (false, false) => TextDirection::Neutral,
    }
}

/// Suggested `/Lang` tag for RTL-dominant text.
pub fn suggested_lang(text: &str) -> Option<&'static str> {
    let mut hebrew = 0usize;
    let mut arabic = 0usize;
    for c in text.chars() {
        match c {
            '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}' => hebrew += 1,
            '\u{0600}'..='\u{06FF}'
            | '\u{0750}'..='\u{077F}'
            | '\u{08A0}'..='\u{08FF}'
            | '\u{FB50}'..='\u{FDFF}'
            | '\u{FE70}'..='\u{FEFF}' => arabic += 1,
            _ => {}
        }
    }
    if hebrew == 0 && arabic == 0 {
        None
    } else if hebrew >= arabic {
        Some("he")
    } else {
        Some("ar")
    }
}

/// Mirror common paired punctuation for RTL visual order.
fn mirror_char(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        _ => c,
    }
}

/// Reverse a string for visual LTR embedding, mirroring paired punctuation.
pub fn visual_reorder_rtl(text: &str) -> String {
    text.chars().rev().map(mirror_char).collect()
}

/// Prepare text for PDF emission under RTL layout.
///
/// - Pure RTL: full visual reverse
/// - Mixed: reverse contiguous RTL runs in place; leave LTR runs as-is
/// - LTR/Neutral: unchanged
pub fn prepare_for_pdf(text: &str) -> String {
    match detect_text_direction(text) {
        TextDirection::Rtl => visual_reorder_rtl(text),
        TextDirection::Mixed => reorder_mixed(text),
        TextDirection::Ltr | TextDirection::Neutral => text.to_string(),
    }
}

fn reorder_mixed(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < chars.len() {
        if is_rtl_char(chars[i]) {
            let start = i;
            i += 1;
            while i < chars.len()
                && (is_rtl_char(chars[i])
                    || (!is_ltr_char(chars[i]) && chars[i].is_whitespace())
                    || matches!(
                        chars[i],
                        ',' | '.' | ':' | ';' | '!' | '?' | '-' | '\'' | '"'
                    ))
            {
                // Don't swallow trailing whitespace into the RTL run
                if chars[i].is_whitespace() {
                    // peek ahead for more RTL
                    let mut j = i + 1;
                    while j < chars.len() && chars[j].is_whitespace() {
                        j += 1;
                    }
                    if j < chars.len() && is_rtl_char(chars[j]) {
                        i = j;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            // trim trailing neutrals that aren't RTL letters from the run end
            let mut end = i;
            while end > start && !is_rtl_char(chars[end - 1]) {
                end -= 1;
            }
            let run: String = chars[start..end].iter().collect();
            out.push_str(&visual_reorder_rtl(&run));
            // emit any neutrals we skipped at the end in original order
            for ch in &chars[end..i] {
                out.push(*ch);
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// True when RTL layout should be used for this string (RTL or mixed with RTL majority).
pub fn prefers_rtl_layout(text: &str) -> bool {
    let mut rtl = 0usize;
    let mut ltr = 0usize;
    for c in text.chars() {
        if is_rtl_char(c) {
            rtl += 1;
        } else if is_ltr_char(c) {
            ltr += 1;
        }
    }
    rtl > 0 && rtl >= ltr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_hebrew_rtl() {
        assert_eq!(detect_text_direction("שלום"), TextDirection::Rtl);
        assert_eq!(suggested_lang("שלום עולם"), Some("he"));
    }

    #[test]
    fn test_detect_arabic_rtl() {
        assert_eq!(detect_text_direction("مرحبا"), TextDirection::Rtl);
        assert_eq!(suggested_lang("مرحبا بالعالم"), Some("ar"));
    }

    #[test]
    fn test_detect_ltr() {
        assert_eq!(detect_text_direction("Hello world"), TextDirection::Ltr);
        assert!(suggested_lang("Hello").is_none());
    }

    #[test]
    fn test_visual_reorder_mirrors_parens() {
        let visual = visual_reorder_rtl("(שלום)");
        // reversed chars with mirrored parens → (םולש)
        assert_eq!(visual, "(םולש)");
    }

    #[test]
    fn test_prepare_pure_rtl() {
        let prepared = prepare_for_pdf("אב");
        assert_eq!(prepared, "בא");
    }

    #[test]
    fn test_prepare_ltr_unchanged() {
        assert_eq!(prepare_for_pdf("Hello"), "Hello");
    }

    #[test]
    fn test_prefers_rtl_layout() {
        assert!(prefers_rtl_layout("שלום"));
        assert!(!prefers_rtl_layout("Hello"));
        assert!(prefers_rtl_layout("OK שלום עולם")); // more RTL letters
        assert!(!prefers_rtl_layout("Hello world and שלום"));
    }

    #[test]
    fn test_mixed_keeps_ltr_run() {
        let prepared = prepare_for_pdf("Hello שלום");
        assert!(prepared.starts_with("Hello"));
        assert!(prepared.contains('ש') || prepared.contains('ם'));
    }
}
