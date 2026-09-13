//! SVG path `d` attribute parsing.

use super::PathOp;
use crate::error::{PdfError, Result};

/// Parse an SVG path `d` attribute into PDF path operators.
///
/// Supports `M/m`, `L/l`, `H/h`, `V/v`, `C/c`, `S/s`, `Q/q`, `T/t`, and `Z/z`.
/// Arc commands (`A/a`) are not supported.
pub fn parse_svg_path(d: &str) -> Result<Vec<PathOp>> {
    let tokens = tokenize_svg_path(d)?;
    let mut ops = Vec::new();
    let mut i = 0usize;
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let mut start_x = 0.0f32;
    let mut start_y = 0.0f32;
    let mut last_cmd = ' ';
    let mut last_ctrl: Option<(f32, f32)> = None; // for S/T reflection

    while i < tokens.len() {
        let cmd = match &tokens[i] {
            SvgToken::Command(c) => {
                i += 1;
                *c
            }
            SvgToken::Number(_) => {
                // Implicit command repetition
                match last_cmd {
                    'M' => 'L',
                    'm' => 'l',
                    c if c.is_ascii_alphabetic() => c,
                    _ => {
                        return Err(PdfError::Svg(format!(
                            "SVG path number without a preceding command near token {i}"
                        )));
                    }
                }
            }
        };

        let relative = cmd.is_ascii_lowercase();
        let cmd_upper = cmd.to_ascii_uppercase();

        match cmd_upper {
            'M' => {
                let (x, y) = read_pair(&tokens, &mut i)?;
                let (x, y) = if relative { (cx + x, cy + y) } else { (x, y) };
                ops.push(PathOp::MoveTo { x, y });
                cx = x;
                cy = y;
                start_x = x;
                start_y = y;
                last_ctrl = None;
                last_cmd = if relative { 'm' } else { 'M' };
                // Subsequent coordinate pairs are implicit LineTo
                while i < tokens.len() && matches!(tokens[i], SvgToken::Number(_)) {
                    let (x, y) = read_pair(&tokens, &mut i)?;
                    let (x, y) = if relative { (cx + x, cy + y) } else { (x, y) };
                    ops.push(PathOp::LineTo { x, y });
                    cx = x;
                    cy = y;
                    last_cmd = if relative { 'l' } else { 'L' };
                }
            }
            'L' => {
                let (x, y) = read_pair(&tokens, &mut i)?;
                let (x, y) = if relative { (cx + x, cy + y) } else { (x, y) };
                ops.push(PathOp::LineTo { x, y });
                cx = x;
                cy = y;
                last_ctrl = None;
                last_cmd = cmd;
            }
            'H' => {
                let x = read_num(&tokens, &mut i)?;
                let x = if relative { cx + x } else { x };
                ops.push(PathOp::LineTo { x, y: cy });
                cx = x;
                last_ctrl = None;
                last_cmd = cmd;
            }
            'V' => {
                let y = read_num(&tokens, &mut i)?;
                let y = if relative { cy + y } else { y };
                ops.push(PathOp::LineTo { x: cx, y });
                cy = y;
                last_ctrl = None;
                last_cmd = cmd;
            }
            'C' => {
                let (x1, y1) = read_pair(&tokens, &mut i)?;
                let (x2, y2) = read_pair(&tokens, &mut i)?;
                let (x3, y3) = read_pair(&tokens, &mut i)?;
                let (x1, y1, x2, y2, x3, y3) = if relative {
                    (cx + x1, cy + y1, cx + x2, cy + y2, cx + x3, cy + y3)
                } else {
                    (x1, y1, x2, y2, x3, y3)
                };
                ops.push(PathOp::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                });
                last_ctrl = Some((x2, y2));
                cx = x3;
                cy = y3;
                last_cmd = cmd;
            }
            'S' => {
                let (x2, y2) = read_pair(&tokens, &mut i)?;
                let (x3, y3) = read_pair(&tokens, &mut i)?;
                let (x2, y2, x3, y3) = if relative {
                    (cx + x2, cy + y2, cx + x3, cy + y3)
                } else {
                    (x2, y2, x3, y3)
                };
                let (x1, y1) = match (last_cmd.to_ascii_uppercase(), last_ctrl) {
                    ('C' | 'S', Some((px, py))) => (2.0 * cx - px, 2.0 * cy - py),
                    _ => (cx, cy),
                };
                ops.push(PathOp::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                });
                last_ctrl = Some((x2, y2));
                cx = x3;
                cy = y3;
                last_cmd = cmd;
            }
            'Q' => {
                let (qx, qy) = read_pair(&tokens, &mut i)?;
                let (x3, y3) = read_pair(&tokens, &mut i)?;
                let (qx, qy, x3, y3) = if relative {
                    (cx + qx, cy + qy, cx + x3, cy + y3)
                } else {
                    (qx, qy, x3, y3)
                };
                let (x1, y1, x2, y2) = quad_to_cubic(cx, cy, qx, qy, x3, y3);
                ops.push(PathOp::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                });
                last_ctrl = Some((qx, qy));
                cx = x3;
                cy = y3;
                last_cmd = cmd;
            }
            'T' => {
                let (x3, y3) = read_pair(&tokens, &mut i)?;
                let (x3, y3) = if relative {
                    (cx + x3, cy + y3)
                } else {
                    (x3, y3)
                };
                let (qx, qy) = match (last_cmd.to_ascii_uppercase(), last_ctrl) {
                    ('Q' | 'T', Some((px, py))) => (2.0 * cx - px, 2.0 * cy - py),
                    _ => (cx, cy),
                };
                let (x1, y1, x2, y2) = quad_to_cubic(cx, cy, qx, qy, x3, y3);
                ops.push(PathOp::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                });
                last_ctrl = Some((qx, qy));
                cx = x3;
                cy = y3;
                last_cmd = cmd;
            }
            'Z' => {
                ops.push(PathOp::Close);
                cx = start_x;
                cy = start_y;
                last_ctrl = None;
                last_cmd = cmd;
            }
            'A' => {
                return Err(PdfError::Svg(
                    "SVG arc commands (A/a) are not supported; convert arcs to cubics first".into(),
                ));
            }
            other => {
                return Err(PdfError::Svg(format!(
                    "Unsupported SVG path command '{other}'"
                )));
            }
        }
    }

    Ok(ops)
}

#[derive(Debug)]
enum SvgToken {
    Command(char),
    Number(f32),
}

fn tokenize_svg_path(d: &str) -> Result<Vec<SvgToken>> {
    let mut tokens = Vec::new();
    let bytes = d.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() || c == ',' {
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() {
            tokens.push(SvgToken::Command(c));
            i += 1;
            continue;
        }
        if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() {
            let start = i;
            if c == '-' || c == '+' {
                i += 1;
            }
            let mut seen_dot = false;
            let mut seen_exp = false;
            while i < bytes.len() {
                let ch = bytes[i] as char;
                if ch.is_ascii_digit() {
                    i += 1;
                } else if ch == '.' && !seen_dot && !seen_exp {
                    seen_dot = true;
                    i += 1;
                } else if (ch == 'e' || ch == 'E') && !seen_exp {
                    seen_exp = true;
                    i += 1;
                    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            let num_str = std::str::from_utf8(&bytes[start..i])
                .map_err(|_| PdfError::Svg("Invalid UTF-8 in SVG path number".into()))?;
            let value: f32 = num_str
                .parse()
                .map_err(|_| PdfError::Svg(format!("Invalid SVG path number '{num_str}'")))?;
            tokens.push(SvgToken::Number(value));
            continue;
        }
        return Err(PdfError::Svg(format!(
            "Unexpected character '{c}' in SVG path"
        )));
    }
    Ok(tokens)
}

fn read_num(tokens: &[SvgToken], i: &mut usize) -> Result<f32> {
    match tokens.get(*i) {
        Some(SvgToken::Number(n)) => {
            *i += 1;
            Ok(*n)
        }
        _ => Err(PdfError::Svg(format!(
            "Expected number in SVG path at token {i}"
        ))),
    }
}

fn read_pair(tokens: &[SvgToken], i: &mut usize) -> Result<(f32, f32)> {
    let x = read_num(tokens, i)?;
    let y = read_num(tokens, i)?;
    Ok((x, y))
}

fn quad_to_cubic(x0: f32, y0: f32, qx: f32, qy: f32, x3: f32, y3: f32) -> (f32, f32, f32, f32) {
    let x1 = x0 + 2.0 / 3.0 * (qx - x0);
    let y1 = y0 + 2.0 / 3.0 * (qy - y0);
    let x2 = x3 + 2.0 / 3.0 * (qx - x3);
    let y2 = y3 + 2.0 / 3.0 * (qy - y3);
    (x1, y1, x2, y2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_svg_path_basic() {
        let ops = parse_svg_path("M10 20 L30 40 Z").unwrap();
        assert_eq!(
            ops,
            vec![
                PathOp::MoveTo { x: 10.0, y: 20.0 },
                PathOp::LineTo { x: 30.0, y: 40.0 },
                PathOp::Close,
            ]
        );
    }

    #[test]
    fn test_parse_svg_path_relative_and_hv() {
        let ops = parse_svg_path("M10,10 h20 v-5 z").unwrap();
        assert_eq!(ops[0], PathOp::MoveTo { x: 10.0, y: 10.0 });
        assert_eq!(ops[1], PathOp::LineTo { x: 30.0, y: 10.0 });
        assert_eq!(ops[2], PathOp::LineTo { x: 30.0, y: 5.0 });
        assert_eq!(ops[3], PathOp::Close);
    }

    #[test]
    fn test_parse_svg_cubic_and_quad() {
        let cubic = parse_svg_path("M0 0 C10 0 10 10 0 10").unwrap();
        assert!(matches!(cubic[1], PathOp::CurveTo { .. }));

        let quad = parse_svg_path("M0 0 Q10 10 20 0").unwrap();
        assert!(matches!(quad[1], PathOp::CurveTo { .. }));
    }

    #[test]
    fn test_parse_svg_arc_rejected() {
        assert!(parse_svg_path("M0 0 A10 10 0 0 1 20 0").is_err());
    }
}
