use regex::{Captures, Regex};

/// Returns true for characters in Unicode blocks that are reliably present
/// in common system fonts (e.g. Arial Unicode.ttf). Characters in the
/// Phonetic Extensions (U+1D00–U+1D7F) and Latin Extended-C (U+2C60–U+2C7F)
/// blocks are often missing, so we avoid them in subscript/superscript
/// conversion and fall back to ASCII bracket notation instead.
fn is_well_supported(ch: char) -> bool {
    let cp = ch as u32;
    !((0x1D00..=0x1D7F).contains(&cp) || (0x2C60..=0x2C7F).contains(&cp))
}

fn to_superscript(text: &str) -> Option<String> {
    let mut out = String::new();
    for ch in text.chars() {
        let mapped = match ch {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            '+' => '⁺',
            '-' => '⁻',
            '=' => '⁼',
            '(' => '⁽',
            ')' => '⁾',
            'a' => 'ᵃ',
            'b' => 'ᵇ',
            'c' => 'ᶜ',
            'd' => 'ᵈ',
            'e' => 'ᵉ',
            'f' => 'ᶠ',
            'g' => 'ᵍ',
            'h' => 'ʰ',
            'i' => 'ⁱ',
            'j' => 'ʲ',
            'k' => 'ᵏ',
            'l' => 'ˡ',
            'm' => 'ᵐ',
            'n' => 'ⁿ',
            'o' => 'ᵒ',
            'p' => 'ᵖ',
            'r' => 'ʳ',
            's' => 'ˢ',
            't' => 'ᵗ',
            'u' => 'ᵘ',
            'v' => 'ᵛ',
            'w' => 'ʷ',
            'x' => 'ˣ',
            'y' => 'ʸ',
            'z' => 'ᶻ',
            'A' => 'ᴬ',
            'B' => 'ᴮ',
            'D' => 'ᴰ',
            'E' => 'ᴱ',
            'G' => 'ᴳ',
            'H' => 'ᴴ',
            'I' => 'ᴵ',
            'J' => 'ᴶ',
            'K' => 'ᴷ',
            'L' => 'ᴸ',
            'M' => 'ᴹ',
            'N' => 'ᴺ',
            'O' => 'ᴼ',
            'P' => 'ᴾ',
            'R' => 'ᴿ',
            'T' => 'ᵀ',
            'U' => 'ᵁ',
            'V' => 'ⱽ',
            'W' => 'ᵂ',
            _ => return None,
        };
        if !is_well_supported(mapped) {
            return None;
        }
        out.push(mapped);
    }
    Some(out)
}

fn to_subscript(text: &str) -> Option<String> {
    // Letter subscripts (ₐ ₑ ₜ …) are often missing from system Unicode fonts
    // and render as '?' after glyph fallback — keep digits/symbols only.
    let mut out = String::new();
    for ch in text.chars() {
        let mapped = match ch {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            '9' => '₉',
            '+' => '₊',
            '-' => '₋',
            '=' => '₌',
            '(' => '₍',
            ')' => '₎',
            _ => return None,
        };
        if !is_well_supported(mapped) {
            return None;
        }
        out.push(mapped);
    }
    Some(out)
}

fn render_operator_with_limits(symbol: &str, lower: &str, upper: &str) -> String {
    // Unicode sub/superscripts only for simple digit/letter limits.
    // Expressions like k=1 contain `=` / multi-glyph forms that often miss
    // glyphs in subset fonts and look broken inline.
    let simple = |s: &str| s.chars().all(|c| c.is_ascii_alphanumeric());
    if simple(lower) && simple(upper) {
        match (to_subscript(lower), to_superscript(upper)) {
            (Some(lo), Some(up)) => format!("{}{}{}", symbol, lo, up),
            _ => format!("{}[{}→{}]", symbol, lower, upper),
        }
    } else {
        format!("{}[{}→{}]", symbol, lower, upper)
    }
}

/// Parse `{...}` with nested-brace support. Returns (inner, bytes including braces).
pub(super) fn parse_brace_group(s: &str) -> Option<(String, usize)> {
    if !s.starts_with('{') {
        return None;
    }
    let mut depth = 0usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((s[1..i].to_string(), i + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// Replace `\frac{num}{den}` using nested-brace-aware parsing (not `[^}]*`).
fn replace_frac_commands(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find("\\frac") {
        out.push_str(&rest[..pos]);
        let after_cmd = &rest[pos + 5..];
        if let Some((num, nlen)) = parse_brace_group(after_cmd) {
            if let Some((den, dlen)) = parse_brace_group(&after_cmd[nlen..]) {
                let num = num.trim();
                let den = den.trim();
                let simple = |t: &str| {
                    !t.is_empty()
                        && t.chars().all(|c| {
                            c.is_ascii_alphanumeric()
                                || c == '+'
                                || c == '-'
                                || c == '('
                                || c == ')'
                        })
                };
                if simple(num) && simple(den) {
                    out.push_str(num);
                    out.push('⁄');
                    out.push_str(den);
                } else {
                    out.push('(');
                    out.push_str(num);
                    out.push_str(")/(");
                    out.push_str(den);
                    out.push(')');
                }
                rest = &after_cmd[nlen + dlen..];
                continue;
            }
        }
        // Unparseable: keep the command literal and advance past it.
        out.push_str("\\frac");
        rest = after_cmd;
    }
    out.push_str(rest);
    out
}

/// Replace `\sqrt[n]{x}` / `\sqrt{x}` with nested-brace awareness.
fn replace_sqrt_commands(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find("\\sqrt") {
        out.push_str(&rest[..pos]);
        let mut after = &rest[pos + 5..];
        let mut index: Option<String> = None;
        if after.starts_with('[') {
            if let Some(end) = after.find(']') {
                index = Some(after[1..end].to_string());
                after = &after[end + 1..];
            }
        }
        if let Some((body, len)) = parse_brace_group(after) {
            match index {
                Some(n) => {
                    out.push('√');
                    out.push('[');
                    out.push_str(&n);
                    out.push_str("](");
                    out.push_str(&body);
                    out.push(')');
                }
                None => {
                    out.push('√');
                    out.push('(');
                    out.push_str(&body);
                    out.push(')');
                }
            }
            rest = &after[len..];
            continue;
        }
        out.push_str("\\sqrt");
        rest = &rest[pos + 5..];
    }
    out.push_str(rest);
    out
}

/// Flatten `\begin{matrix|bmatrix|pmatrix|vmatrix}...\end{...}` (possibly multi-line).
pub(crate) fn flatten_math_environments(expr: &str) -> String {
    let mut s = expr.to_string();
    for env in ["bmatrix", "pmatrix", "vmatrix", "matrix"] {
        let open = format!("\\begin{{{}}}", env);
        let close = format!("\\end{{{}}}", env);
        while let Some(start) = s.find(&open) {
            let body_start = start + open.len();
            let Some(rel_end) = s[body_start..].find(&close) else {
                break;
            };
            let body = s[body_start..body_start + rel_end].trim();
            let rows: Vec<String> = body
                .split("\\\\")
                .map(|r| {
                    r.trim()
                        .split('&')
                        .map(str::trim)
                        .filter(|c| !c.is_empty())
                        .collect::<Vec<_>>()
                        .join("  ")
                })
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty())
                .collect();
            let (left, right) = match env {
                "vmatrix" => ("|", "|"),
                "pmatrix" => ("(", ")"),
                _ => ("[", "]"),
            };
            let replacement = if rows.is_empty() {
                format!("{}{}", left, right)
            } else if rows.len() == 1 {
                format!("{} {} {}", left, rows[0], right)
            } else {
                format!("{} {} {}", left, rows.join(" ; "), right)
            };
            let end = body_start + rel_end + close.len();
            s.replace_range(start..end, &replacement);
        }
    }
    s
}

/// Apply command→replacement pairs longest-first so `\le` cannot eat `\left`.
fn apply_commands_longest_first(s: &str, commands: &[(&str, &str)]) -> String {
    let mut cmds: Vec<(&str, &str)> = commands.to_vec();
    cmds.sort_by_key(|(cmd, _)| std::cmp::Reverse(cmd.len()));
    let mut out = s.to_string();
    for (cmd, replacement) in cmds {
        out = out.replace(cmd, replacement);
    }
    out
}

/// Convert LaTeX-like math notation to readable text for PDF rendering.
/// Since Type1 fonts don't support full LaTeX glyph rendering, we convert
/// common math commands to their text/symbol equivalents.
pub(crate) fn render_math_text(expr: &str) -> String {
    let mut s = flatten_math_environments(expr);

    // Greek letters
    let greek = [
        ("\\alpha", "\u{03B1}"),
        ("\\beta", "\u{03B2}"),
        ("\\gamma", "\u{03B3}"),
        ("\\delta", "\u{03B4}"),
        ("\\epsilon", "\u{03B5}"),
        ("\\zeta", "\u{03B6}"),
        ("\\eta", "\u{03B7}"),
        ("\\theta", "\u{03B8}"),
        ("\\iota", "\u{03B9}"),
        ("\\kappa", "\u{03BA}"),
        ("\\lambda", "\u{03BB}"),
        ("\\mu", "\u{03BC}"),
        ("\\nu", "\u{03BD}"),
        ("\\xi", "\u{03BE}"),
        ("\\pi", "\u{03C0}"),
        ("\\rho", "\u{03C1}"),
        ("\\sigma", "\u{03C3}"),
        ("\\tau", "\u{03C4}"),
        ("\\upsilon", "\u{03C5}"),
        ("\\phi", "\u{03C6}"),
        ("\\chi", "\u{03C7}"),
        ("\\psi", "\u{03C8}"),
        ("\\omega", "\u{03C9}"),
        ("\\Alpha", "A"),
        ("\\Beta", "B"),
        ("\\Gamma", "\u{0393}"),
        ("\\Delta", "\u{0394}"),
        ("\\Theta", "\u{0398}"),
        ("\\Lambda", "\u{039B}"),
        ("\\Xi", "\u{039E}"),
        ("\\Pi", "\u{03A0}"),
        ("\\Sigma", "\u{03A3}"),
        ("\\Phi", "\u{03A6}"),
        ("\\Psi", "\u{03A8}"),
        ("\\Omega", "\u{03A9}"),
        ("\\ell", "ℓ"),
    ];

    // Math operators and symbols — applied longest-first later.
    let operators = [
        ("\\infty", "∞"),
        ("\\infinity", "∞"),
        ("\\pm", "±"),
        ("\\mp", "∓"),
        ("\\times", "×"),
        ("\\cdot", "·"),
        ("\\div", "÷"),
        ("\\neq", "≠"),
        ("\\ne", "≠"),
        ("\\leq", "≤"),
        ("\\le", "≤"),
        ("\\geq", "≥"),
        ("\\ge", "≥"),
        ("\\approx", "≈"),
        ("\\sim", "∼"),
        ("\\equiv", "≡"),
        ("\\propto", "∝"),
        ("\\rightarrow", "→"),
        ("\\leftarrow", "←"),
        ("\\to", "→"),
        ("\\Rightarrow", "⇒"),
        ("\\Leftarrow", "⇐"),
        ("\\leftrightarrow", "↔"),
        ("\\forall", "∀"),
        ("\\exists", "∃"),
        ("\\notin", "∉"),
        ("\\in", "∈"),
        ("\\subseteq", "⊆"),
        ("\\supseteq", "⊇"),
        ("\\subsetneq", "⊊"),
        ("\\supsetneq", "⊋"),
        ("\\subset", "⊂"),
        ("\\supset", "⊃"),
        ("\\cup", "∪"),
        ("\\cap", "∩"),
        ("\\wedge", "∧"),
        ("\\land", "∧"),
        ("\\vee", "∨"),
        ("\\lor", "∨"),
        ("\\neg", "¬"),
        ("\\lnot", "¬"),
        ("\\iff", "⇔"),
        ("\\implies", "⇒"),
        ("\\therefore", "∴"),
        ("\\because", "∵"),
        ("\\emptyset", "∅"),
        ("\\nabla", "∇"),
        ("\\partial", "∂"),
        ("\\ldots", "..."),
        ("\\cdots", "..."),
        ("\\dots", "..."),
        ("\\quad", "  "),
        ("\\qquad", "    "),
        ("\\,", " "),
        ("\\;", " "),
        ("\\!", ""),
        ("\\left", ""),
        ("\\right", ""),
        ("\\big", ""),
        ("\\Big", ""),
        ("\\bigg", ""),
        ("\\Bigg", ""),
        ("\\argmax", "argmax"),
        ("\\argmin", "argmin"),
        ("\\arg\\max", "argmax"),
        ("\\arg\\min", "argmin"),
        ("\\|", "‖"),
        ("\\vert", "|"),
        ("\\lvert", "|"),
        ("\\rvert", "|"),
    ];

    // Apply Greek letter replacements (longer patterns first to avoid partial matches)
    s = apply_commands_longest_first(&s, &greek);

    // Common LaTeX variants and blackboard-bold sets
    s = s.replace("\\not\\in", "∉");
    s = s.replace("\\mathbb{R}", "ℝ");
    s = s.replace("\\mathbb{N}", "ℕ");
    s = s.replace("\\mathbb{Z}", "ℤ");
    s = s.replace("\\mathbb{Q}", "ℚ");
    s = s.replace("\\mathbb{C}", "ℂ");
    s = s.replace("\\mathbb{P}", "ℙ");
    s = s.replace("\\mathbb{H}", "ℍ");

    // Nested-brace-aware fraction / root conversion (iterative for nesting).
    loop {
        let next = replace_frac_commands(&s);
        if next == s {
            break;
        }
        s = next;
    }
    loop {
        let next = replace_sqrt_commands(&s);
        if next == s {
            break;
        }
        s = next;
    }

    // Handle \sum, \prod, \int with limits
    let sum_re = Regex::new(r"\\sum_\{([^}]*)\}\^\{([^}]*)\}").unwrap();
    s = sum_re
        .replace_all(&s, |caps: &Captures| {
            render_operator_with_limits("∑", &caps[1], &caps[2])
        })
        .to_string();
    let sum_re_simple = Regex::new(r"\\sum_([^\s\^_{}]+)\^([^\s\^_{}]+)").unwrap();
    s = sum_re_simple
        .replace_all(&s, |caps: &Captures| {
            render_operator_with_limits("∑", &caps[1], &caps[2])
        })
        .to_string();
    s = s.replace("\\sum", "∑");

    let prod_re = Regex::new(r"\\prod_\{([^}]*)\}\^\{([^}]*)\}").unwrap();
    s = prod_re
        .replace_all(&s, |caps: &Captures| {
            render_operator_with_limits("∏", &caps[1], &caps[2])
        })
        .to_string();
    let prod_re_simple = Regex::new(r"\\prod_([^\s\^_{}]+)\^([^\s\^_{}]+)").unwrap();
    s = prod_re_simple
        .replace_all(&s, |caps: &Captures| {
            render_operator_with_limits("∏", &caps[1], &caps[2])
        })
        .to_string();
    s = s.replace("\\prod", "∏");

    let int_re = Regex::new(r"\\int_\{([^}]*)\}\^\{([^}]*)\}").unwrap();
    s = int_re
        .replace_all(&s, |caps: &Captures| {
            render_operator_with_limits("∫", &caps[1], &caps[2])
        })
        .to_string();
    let int_re_simple = Regex::new(r"\\int_([^\s\^_{}]+)\^([^\s\^_{}]+)").unwrap();
    s = int_re_simple
        .replace_all(&s, |caps: &Captures| {
            render_operator_with_limits("∫", &caps[1], &caps[2])
        })
        .to_string();
    s = s.replace("\\int", "∫");

    let lim_re = Regex::new(r"\\lim_\{([^}]*)\}").unwrap();
    s = lim_re.replace_all(&s, "lim($1)").to_string();
    let lim_re_simple = Regex::new(r"\\lim_([^\s\^_{}]+)").unwrap();
    s = lim_re_simple.replace_all(&s, "lim($1)").to_string();
    s = s.replace("\\lim", "lim");

    // Handle superscript ^{x} -> ˣ and subscript _{x} -> ₓ (fallback to ASCII markers)
    let sup_re = Regex::new(r"\^\{([^}]*)\}").unwrap();
    s = sup_re
        .replace_all(&s, |caps: &Captures| {
            to_superscript(&caps[1]).unwrap_or_else(|| format!("^({})", &caps[1]))
        })
        .to_string();
    let sup_simple_re = Regex::new(r"\^([A-Za-z0-9+\-*/=])").unwrap();
    s = sup_simple_re
        .replace_all(&s, |caps: &Captures| {
            to_superscript(&caps[1]).unwrap_or_else(|| format!("^({})", &caps[1]))
        })
        .to_string();

    let sub_re = Regex::new(r"_\{([^}]*)\}").unwrap();
    s = sub_re
        .replace_all(&s, |caps: &Captures| {
            to_subscript(&caps[1]).unwrap_or_else(|| format!("_({})", &caps[1]))
        })
        .to_string();
    let sub_simple_re = Regex::new(r"_([A-Za-z0-9])").unwrap();
    s = sub_simple_re
        .replace_all(&s, |caps: &Captures| {
            to_subscript(&caps[1]).unwrap_or_else(|| format!("_({})", &caps[1]))
        })
        .to_string();

    // Handle \text{...} -> ...
    let text_re = Regex::new(r"\\text\{([^}]*)\}").unwrap();
    s = text_re.replace_all(&s, "$1").to_string();

    // Handle \mathbf{...}, \mathrm{...}, \mathit{...} -> content
    let mathfmt_re = Regex::new(r"\\math[a-z]+\{([^}]*)\}").unwrap();
    s = mathfmt_re.replace_all(&s, "$1").to_string();

    // Handle \hat{x}, \bar{x}, \vec{x}, \tilde{x}
    let hat_re = Regex::new(r"\\hat\{([^}]*)\}").unwrap();
    s = hat_re.replace_all(&s, "$1\u{0302}").to_string();
    let bar_re = Regex::new(r"\\bar\{([^}]*)\}").unwrap();
    s = bar_re.replace_all(&s, "$1\u{0304}").to_string();
    let vec_re = Regex::new(r"\\vec\{([^}]*)\}").unwrap();
    s = vec_re.replace_all(&s, "vec($1)").to_string();
    let tilde_re = Regex::new(r"\\tilde\{([^}]*)\}").unwrap();
    s = tilde_re.replace_all(&s, "$1\u{0303}").to_string();

    // Handle \log, \ln, \sin, \cos, \tan, \exp (longest first for sinh/cosh/tanh)
    for func in &[
        "sinh", "cosh", "tanh", "log", "ln", "sin", "cos", "tan", "cot", "sec", "csc", "exp",
        "min", "max", "det", "dim", "arg",
    ] {
        let cmd = format!("\\{}", func);
        s = s.replace(&cmd, func);
    }

    // Apply generic operator replacements late to avoid prefix collisions
    // such as \in replacing the start of \int. Longest-first protects \left/\leq.
    s = apply_commands_longest_first(&s, &operators);

    // Strip remaining braces
    s = s.replace(['{', '}'], "");

    // Clean up multiple spaces
    let multi_space = Regex::new(r"  +").unwrap();
    s = multi_space.replace_all(&s, " ").to_string();

    s.trim().to_string()
}

/// Escape a PDF string literal (for ASCII-only text)
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
fn normalize_for_base14_font(text: &str) -> String {
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

pub(super) fn use_base14_normalization() -> bool {
    std::env::var("PDFRS_BASE14_NORMALIZE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Encode text for PDF - uses UTF-16BE hex encoding for unicode, literal string for ASCII
pub(super) fn encode_pdf_text(text: &str) -> String {
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
