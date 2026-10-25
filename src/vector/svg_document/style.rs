//! SVG paint/color parsing and path helpers.

use super::context::{SvgElement, SvgPaint};
use crate::pdf_generator::Color;
use crate::vector::PathOp;

pub(super) fn rounded_rect_path_ops(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<PathOp> {
    let k = 0.552_284_8_f32 * r;
    vec![
        PathOp::MoveTo { x: x + r, y: y + h },
        PathOp::LineTo {
            x: x + w - r,
            y: y + h,
        },
        PathOp::CurveTo {
            x1: x + w - r + k,
            y1: y + h,
            x2: x + w,
            y2: y + h - r + k,
            x3: x + w,
            y3: y + h - r,
        },
        PathOp::LineTo { x: x + w, y: y + r },
        PathOp::CurveTo {
            x1: x + w,
            y1: y + r - k,
            x2: x + w - r + k,
            y2: y,
            x3: x + w - r,
            y3: y,
        },
        PathOp::LineTo { x: x + r, y },
        PathOp::CurveTo {
            x1: x + r - k,
            y1: y,
            x2: x,
            y2: y + r - k,
            x3: x,
            y3: y + r,
        },
        PathOp::LineTo { x, y: y + h - r },
        PathOp::CurveTo {
            x1: x,
            y1: y + h - r + k,
            x2: x + r - k,
            y2: y + h,
            x3: x + r,
            y3: y + h,
        },
        PathOp::Close,
    ]
}

pub(super) fn attr_f32(el: &SvgElement, key: &str, default: f32) -> f32 {
    el.attrs
        .get(key)
        .and_then(|s| {
            s.trim()
                .trim_end_matches("px")
                .trim_end_matches("pt")
                .parse::<f32>()
                .ok()
        })
        .unwrap_or(default)
}

pub(super) fn parse_paint(s: &str) -> SvgPaint {
    let trimmed = s.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "none" => SvgPaint::None,
        "inherit" | "currentcolor" | "" => SvgPaint::Inherit,
        "black" => SvgPaint::Color(Color::black()),
        "white" => SvgPaint::Color(Color::rgb(1.0, 1.0, 1.0)),
        "red" => SvgPaint::Color(Color::rgb(1.0, 0.0, 0.0)),
        "green" => SvgPaint::Color(Color::rgb(0.0, 0.5, 0.0)),
        "blue" => SvgPaint::Color(Color::rgb(0.0, 0.0, 1.0)),
        "yellow" => SvgPaint::Color(Color::rgb(1.0, 1.0, 0.0)),
        "cyan" => SvgPaint::Color(Color::rgb(0.0, 1.0, 1.0)),
        "magenta" => SvgPaint::Color(Color::rgb(1.0, 0.0, 1.0)),
        "gray" | "grey" => SvgPaint::Color(Color::rgb(0.5, 0.5, 0.5)),
        _ if trimmed.starts_with('#') => {
            if let Some(c) = parse_hex_color(trimmed) {
                SvgPaint::Color(c)
            } else {
                SvgPaint::Color(Color::black())
            }
        }
        _ if trimmed.starts_with("rgb(") && trimmed.ends_with(')') => {
            let inner = &trimmed[4..trimmed.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 3
                && let (Ok(r), Ok(g), Ok(b)) = (
                    parts[0].parse::<u8>(),
                    parts[1].parse::<u8>(),
                    parts[2].parse::<u8>(),
                )
            {
                SvgPaint::Color(Color::rgb(
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    b as f32 / 255.0,
                ))
            } else {
                SvgPaint::Color(Color::black())
            }
        }
        _ => SvgPaint::Color(Color::black()),
    }
}

pub(super) fn parse_hex_color(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#')?;
    let (r, g, b) = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            (r, g, b)
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b)
        }
        _ => return None,
    };
    Some(Color::rgb(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
    ))
}
