//! PDF string escaping and Base-14 text encoding.

pub(crate) fn escape_pdf_string(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

/// Normalize text for Base-14 fonts (Helvetica/Courier family).
///
/// This is a compatibility fallback mode that can be enabled when a viewer
/// cannot render UTF-16 text with Base-14 fonts.
pub(super) fn normalize_for_base14_font(text: &str) -> String {
    let mut out = String::new();

    for ch in text.chars() {
        match ch {
            // Fast path
            c if c.is_ascii() => out.push(c),

            // Math operators / relations
            '∞' => out.push_str("infinity"),
            '∑' => out.push_str("sum"),
            '∏' => out.push_str("prod"),
            '∫' => out.push_str("int"),
            '∂' => out.push_str("partial"),
            '∇' => out.push_str("nabla"),
            '√' => out.push_str("sqrt"),
            '≈' => out.push_str("~="),
            '≠' => out.push_str("!="),
            '≤' => out.push_str("<="),
            '≥' => out.push_str(">="),
            '±' => out.push_str("+/-"),
            '×' => out.push('*'),
            '÷' => out.push('/'),
            '∈' => out.push_str(" in "),
            '∉' => out.push_str(" not-in "),
            '∩' => out.push_str(" cap "),
            '∪' => out.push_str(" cup "),
            '⊂' => out.push_str(" subset "),
            '⊃' => out.push_str(" superset "),
            '⊆' => out.push_str(" subseteq "),
            '⊇' => out.push_str(" superseteq "),
            '⊊' => out.push_str(" subsetneq "),
            '⊋' => out.push_str(" supersetneq "),
            '∀' => out.push_str("forall"),
            '∃' => out.push_str("exists"),
            '∧' => out.push_str(" and "),
            '∨' => out.push_str(" or "),
            '¬' => out.push_str(" not "),
            '∴' => out.push_str(" therefore "),
            '∵' => out.push_str(" because "),

            // Greek letters commonly produced by render_math_text
            'α' => out.push_str("alpha"),
            'β' => out.push_str("beta"),
            'γ' => out.push_str("gamma"),
            'δ' => out.push_str("delta"),
            'ε' => out.push_str("epsilon"),
            'θ' => out.push_str("theta"),
            'λ' => out.push_str("lambda"),
            'μ' => out.push_str("mu"),
            'π' => out.push_str("pi"),
            'σ' => out.push_str("sigma"),
            'φ' => out.push_str("phi"),
            'ω' => out.push_str("omega"),
            'Γ' => out.push_str("Gamma"),
            'Δ' => out.push_str("Delta"),
            'Θ' => out.push_str("Theta"),
            'Λ' => out.push_str("Lambda"),
            'Π' => out.push_str("Pi"),
            'Σ' => out.push_str("Sigma"),
            'Φ' => out.push_str("Phi"),
            'Ω' => out.push_str("Omega"),
            'ℝ' => out.push('R'),
            'ℕ' => out.push('N'),
            'ℤ' => out.push('Z'),
            'ℚ' => out.push('Q'),
            'ℂ' => out.push('C'),
            'ℙ' => out.push('P'),
            'ℍ' => out.push('H'),

            // Soft hyphen: render as a visible ASCII hyphen in the legacy
            // base-14 fallback so the break point is still observable.
            '\u{00AD}' => out.push('-'),

            // Currency
            '€' => out.push_str("EUR"),
            '£' => out.push_str("GBP"),
            '¥' => out.push_str("JPY"),
            '₹' => out.push_str("INR"),
            '₽' => out.push_str("RUB"),
            '₩' => out.push_str("KRW"),
            '₿' => out.push_str("BTC"),

            // Arrows
            '←' => out.push_str("<-"),
            '→' => out.push_str("->"),
            '↔' => out.push_str("<->"),
            '⇐' => out.push_str("<="),
            '⇒' => out.push_str("=>"),
            '⇔' => out.push_str("<=>"),

            // Superscripts / subscripts seen in examples
            '²' => out.push_str("^2"),
            '³' => out.push_str("^3"),
            '₀' => out.push_str("_0"),
            '₁' => out.push_str("_1"),
            '₂' => out.push_str("_2"),
            '₃' => out.push_str("_3"),
            '₄' => out.push_str("_4"),
            '₅' => out.push_str("_5"),
            '₆' => out.push_str("_6"),
            '₇' => out.push_str("_7"),
            '₈' => out.push_str("_8"),
            '₉' => out.push_str("_9"),

            // Fallback: keep visibility instead of rendering blank glyphs
            other => out.push_str(&format!("[U+{:04X}]", other as u32)),
        }
    }

    out
}

pub(crate) fn use_base14_normalization() -> bool {
    std::env::var("PDFRS_BASE14_NORMALIZE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Encode text for PDF - uses UTF-16BE hex encoding for unicode, literal string for ASCII
pub(crate) fn encode_pdf_text(text: &str) -> String {
    let normalized = if use_base14_normalization() {
        normalize_for_base14_font(text)
    } else {
        text.to_string()
    };

    // Check if text contains any non-ASCII characters
    let has_unicode = !normalized.is_ascii();

    if !has_unicode {
        // Pure ASCII - use literal string format
        format!("({})", escape_pdf_string(&normalized))
    } else {
        // Contains unicode - use UTF-16BE hex encoding with BOM
        let mut utf16be_bytes = Vec::new();

        // Add BOM (Big Endian)
        utf16be_bytes.push(0xFE);
        utf16be_bytes.push(0xFF);

        // Encode each character as UTF-16BE
        for c in normalized.chars() {
            let mut code = c as u32;
            if code < 0x10000 {
                // BMP character - single UTF-16 code unit
                utf16be_bytes.push((code >> 8) as u8);
                utf16be_bytes.push((code & 0xFF) as u8);
            } else {
                // Surrogate pair for characters beyond BMP
                code -= 0x10000;
                let high_surrogate = 0xD800 + ((code >> 10) & 0x3FF);
                let low_surrogate = 0xDC00 + (code & 0x3FF);
                utf16be_bytes.push((high_surrogate >> 8) as u8);
                utf16be_bytes.push((high_surrogate & 0xFF) as u8);
                utf16be_bytes.push((low_surrogate >> 8) as u8);
                utf16be_bytes.push((low_surrogate & 0xFF) as u8);
            }
        }

        // Format as hex string
        let hex_string: String = utf16be_bytes.iter().map(|b| format!("{:02X}", b)).collect();

        format!("<{}>", hex_string)
    }
}
