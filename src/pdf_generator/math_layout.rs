//! Lightweight display-math layout for PDF (stacked fractions, operator limits).
//!
//! This is intentionally a small subset of LaTeX — enough for common
//! Markdown math blocks without pulling in a full TeX engine.

use regex::Regex;

use super::text_support::render_math_text;

/// A laid-out piece of display mathematics.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum MathPiece {
    /// Ordinary run of already-rendered math text.
    Text(String),
    /// Large operator with limits (∑/∏: above/below; ∫: side scripts).
    Operator {
        symbol: char,
        lower: String,
        upper: String,
        /// When true, place limits to the right (integrals). Otherwise above/below.
        side_limits: bool,
    },
    /// Stacked fraction with a horizontal rule.
    Fraction {
        numerator: String,
        denominator: String,
    },
}

/// Parse a LaTeX-like math expression into display pieces.
pub(super) fn parse_display_math(expr: &str) -> Vec<MathPiece> {
    let mut s = expr.trim().to_string();
    if s.is_empty() {
        return Vec::new();
    }

    // Normalize common spacing commands early so token splits stay simple.
    s = s.replace("\\,", " ");
    s = s.replace("\\;", " ");
    s = s.replace("\\!", "");
    s = s.replace("\\quad", "  ");
    s = s.replace("\\qquad", "   ");
    s = s.replace("\\left", "");
    s = s.replace("\\right", "");

    let mut pieces = Vec::new();
    let mut i = 0;
    let bytes = s.as_bytes();

    while i < bytes.len() {
        // Skip whitespace but keep a single space as text when between tokens.
        if bytes[i].is_ascii_whitespace() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if !pieces.is_empty() {
                pieces.push(MathPiece::Text(" ".to_string()));
            }
            let _ = start;
            continue;
        }

        let rest = &s[i..];

        if let Some((piece, consumed)) = try_parse_operator(rest) {
            pieces.push(piece);
            i += consumed;
            continue;
        }

        if let Some((piece, consumed)) = try_parse_fraction(rest) {
            pieces.push(piece);
            i += consumed;
            continue;
        }

        // Ordinary text until next special command or end.
        let next_special = find_next_special(rest);
        let chunk = &rest[..next_special];
        if !chunk.is_empty() {
            let rendered = render_math_text(chunk);
            if !rendered.is_empty() {
                pieces.push(MathPiece::Text(rendered));
            }
            i += chunk.len();
        } else {
            // Unknown backslash command: take one token and render it.
            let tok_end = rest
                .char_indices()
                .skip(1)
                .find(|(_, c)| c.is_whitespace() || *c == '\\' || *c == '{' || *c == '^' || *c == '_')
                .map(|(idx, _)| idx)
                .unwrap_or(rest.len())
                .max(1);
            let tok = &rest[..tok_end];
            let rendered = render_math_text(tok);
            if !rendered.is_empty() {
                pieces.push(MathPiece::Text(rendered));
            }
            i += tok_end;
        }
    }

    // Merge adjacent text pieces and drop empty ones.
    coalesce_text_pieces(pieces)
}

fn find_next_special(s: &str) -> usize {
    let markers = ["\\sum", "\\prod", "\\int", "\\frac"];
    let mut best = s.len();
    for m in markers {
        if let Some(pos) = s.find(m) {
            best = best.min(pos);
        }
    }
    // Also stop before a lone backslash that starts a command after content.
    if let Some(pos) = s.find('\\') {
        // If the backslash is itself one of the markers, keep that.
        // Otherwise if it appears before best and is not part of already-handled
        // content, we still want ordinary render_math_text to handle it — so
        // only stop at markers for piece boundaries.
        let _ = pos;
    }
    best
}

fn try_parse_operator(s: &str) -> Option<(MathPiece, usize)> {
    let (symbol, side_limits, cmd_len) = if s.starts_with("\\sum") {
        ('∑', false, 4)
    } else if s.starts_with("\\prod") {
        ('∏', false, 5)
    } else if s.starts_with("\\int") {
        ('∫', true, 4)
    } else {
        return None;
    };

    let mut idx = cmd_len;
    let (lower, upper, consumed) = parse_limits(&s[idx..]);
    idx += consumed;

    Some((
        MathPiece::Operator {
            symbol,
            lower: render_math_text(&lower),
            upper: render_math_text(&upper),
            side_limits,
        },
        idx,
    ))
}

fn try_parse_fraction(s: &str) -> Option<(MathPiece, usize)> {
    if !s.starts_with("\\frac") {
        return None;
    }
    let mut idx = 5; // \frac
    let (num, nlen) = parse_brace_group(&s[idx..])?;
    idx += nlen;
    let (den, dlen) = parse_brace_group(&s[idx..])?;
    idx += dlen;
    Some((
        MathPiece::Fraction {
            numerator: render_math_text(&num),
            denominator: render_math_text(&den),
        },
        idx,
    ))
}

/// Parse `_lower^upper`, `_{lower}^{upper}`, or mixed forms. Returns (lower, upper, bytes_consumed).
fn parse_limits(s: &str) -> (String, String, usize) {
    let mut idx = 0;
    let mut lower = String::new();
    let mut upper = String::new();

    for _ in 0..2 {
        let rest = &s[idx..];
        if rest.starts_with("_{") {
            if let Some((content, len)) = parse_brace_group(&rest[1..]) {
                lower = content;
                idx += 1 + len;
                continue;
            }
        }
        if rest.starts_with("^{") {
            if let Some((content, len)) = parse_brace_group(&rest[1..]) {
                upper = content;
                idx += 1 + len;
                continue;
            }
        }
        if let Some(caps) = Regex::new(r"^_([A-Za-z0-9+\-*/=]+)").unwrap().captures(rest) {
            lower = caps[1].to_string();
            idx += caps.get(0).unwrap().end();
            continue;
        }
        if let Some(caps) = Regex::new(r"^\^([A-Za-z0-9+\-*/=]+)").unwrap().captures(rest) {
            upper = caps[1].to_string();
            idx += caps.get(0).unwrap().end();
            continue;
        }
        break;
    }

    (lower, upper, idx)
}

/// Parse `{...}` with simple nested-brace support. Returns (inner, bytes including braces).
fn parse_brace_group(s: &str) -> Option<(String, usize)> {
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

fn coalesce_text_pieces(pieces: Vec<MathPiece>) -> Vec<MathPiece> {
    let mut out: Vec<MathPiece> = Vec::new();
    for piece in pieces {
        match piece {
            MathPiece::Text(t) if t.is_empty() => {}
            MathPiece::Text(t) => {
                if let Some(MathPiece::Text(prev)) = out.last_mut() {
                    prev.push_str(&t);
                } else {
                    out.push(MathPiece::Text(t));
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Estimated width of a display piece at the given body font size.
pub(super) fn piece_width(
    piece: &MathPiece,
    font_size: f32,
    measure: &dyn Fn(&str, f32) -> f32,
) -> f32 {
    match piece {
        MathPiece::Text(t) => measure(t, font_size),
        MathPiece::Operator {
            symbol,
            lower,
            upper,
            side_limits,
        } => {
            let op_size = font_size * 1.55;
            let script = font_size * 0.62;
            let sym = measure(&symbol.to_string(), op_size);
            if *side_limits {
                let lim_w = measure(lower, script).max(measure(upper, script));
                sym + 2.0 + lim_w
            } else {
                let lim_w = measure(lower, script).max(measure(upper, script));
                sym.max(lim_w)
            }
        }
        MathPiece::Fraction {
            numerator,
            denominator,
        } => {
            let script = font_size * 0.85;
            measure(numerator, script).max(measure(denominator, script)) + 6.0
        }
    }
}

/// Total height of a display line of pieces (above + below math axis).
pub(super) fn line_height_for_pieces(pieces: &[MathPiece], font_size: f32) -> f32 {
    let mut ascent = font_size * 0.75;
    let mut descent = font_size * 0.35;
    for piece in pieces {
        match piece {
            MathPiece::Text(_) => {}
            MathPiece::Operator { side_limits, .. } => {
                if *side_limits {
                    ascent = ascent.max(font_size * 1.1);
                    descent = descent.max(font_size * 0.55);
                } else {
                    ascent = ascent.max(font_size * 1.85);
                    descent = descent.max(font_size * 1.15);
                }
            }
            MathPiece::Fraction { .. } => {
                ascent = ascent.max(font_size * 1.15);
                descent = descent.max(font_size * 1.15);
            }
        }
    }
    ascent + descent + 4.0
}

/// Flatten display pieces to a readable single-line string (for extraction / a11y).
pub(super) fn pieces_to_plain_text(pieces: &[MathPiece]) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            MathPiece::Text(t) => out.push_str(t),
            MathPiece::Operator {
                symbol,
                lower,
                upper,
                ..
            } => {
                out.push(*symbol);
                if !lower.is_empty() || !upper.is_empty() {
                    out.push('[');
                    out.push_str(lower);
                    out.push('→');
                    out.push_str(upper);
                    out.push(']');
                }
            }
            MathPiece::Fraction {
                numerator,
                denominator,
            } => {
                out.push('(');
                out.push_str(numerator);
                out.push(')');
                out.push('/');
                out.push('(');
                out.push_str(denominator);
                out.push(')');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_integral_fraction_sum() {
        let pieces = parse_display_math(r"\int_{0}^{1} x^{2}\, dx = \frac{1}{3}");
        assert!(
            pieces.iter().any(|p| matches!(p, MathPiece::Operator { symbol: '∫', .. })),
            "{:?}",
            pieces
        );
        assert!(
            pieces.iter().any(|p| matches!(
                p,
                MathPiece::Fraction {
                    numerator,
                    denominator
                } if numerator == "1" && denominator == "3"
            )),
            "{:?}",
            pieces
        );
    }

    #[test]
    fn parses_sum_with_complex_lower_limit() {
        let pieces = parse_display_math(r"\sum_{k=1}^{n} k = \frac{n(n+1)}{2}");
        assert!(
            pieces.iter().any(|p| matches!(
                p,
                MathPiece::Operator {
                    symbol: '∑',
                    lower,
                    upper,
                    side_limits: false
                } if lower.contains('k') && upper.contains('n')
            )),
            "{:?}",
            pieces
        );
    }

    #[test]
    fn plain_text_is_readable() {
        let pieces = parse_display_math(r"\sum_{k=1}^{n} k = \frac{n(n+1)}{2}");
        let plain = pieces_to_plain_text(&pieces);
        assert!(plain.contains('∑'), "{}", plain);
        assert!(plain.contains('→') || plain.contains('k'), "{}", plain);
        assert!(plain.contains('/') || plain.contains("n+1"), "{}", plain);
    }
}
