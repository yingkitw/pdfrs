//! Rewriting a content stream so that redacted text is removed.
//!
//! The rewriter walks the content stream, tracks the current transformation
//! matrix and text position, and masks the text-showing operators that fall
//! inside a redaction region.

use std::collections::HashMap;

use super::document::XObjectMap;
use super::model::RedactionStyle;
use crate::search::{Rect, TjItem, extract_tj_array};

/// Rewrite one content stream. Returns the rewritten stream plus the names of
/// image XObjects whose `Do` calls were removed.
pub(super) fn rewrite_stream(
    src: &str,
    regions: &[Rect],
    fonts: &HashMap<String, crate::search::FontMetrics>,
    style: RedactionStyle,
    image_xobjects: &XObjectMap,
) -> (String, Vec<String>) {
    let tokens = crate::search::tokenize(src);
    let mut i = 0;
    let mut operands: Vec<f32> = Vec::new();
    let mut pending_name: Option<String> = None;
    let mut text_matrix = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut text_line_matrix = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut font_size = 12.0f32;
    let mut current_metrics: Option<crate::search::FontMetrics> = None;
    // CTM stack for tracking graphics state (for image XObject placement).
    let mut ctm_stack: Vec<[f32; 6]> = Vec::new();
    let mut ctm = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut out = String::new();
    let mut removed_images: Vec<String> = Vec::new();

    // Emit a pending `/Name` operand plus numeric operands and the operator.
    macro_rules! emit {
        ($op:expr) => {{
            if let Some(name) = pending_name.take() {
                out.push('/');
                out.push_str(&name);
                out.push(' ');
            }
            for n in &operands {
                out.push_str(&fmt_f(*n));
                out.push(' ');
            }
            out.push_str($op);
            out.push('\n');
        }};
    }

    while i < tokens.len() {
        let t = &tokens[i];
        if let Some(name) = t.strip_prefix('/') {
            pending_name = Some(name.to_string());
            i += 1;
            continue;
        }
        if let Ok(n) = t.parse::<f32>() {
            operands.push(n);
            i += 1;
            continue;
        }
        let op = t.as_str();
        match op {
            "BT" => {
                text_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                text_line_matrix = text_matrix;
                out.push_str("BT\n");
            }
            "ET" => {
                // Append black-box overlays for every region on this page.
                if style == RedactionStyle::BlackBox {
                    for r in regions {
                        out.push_str(&format!(
                            "q\n0 0 0 rg\n{} {} {} {} re\nf\nQ\n",
                            fmt_f(r.x),
                            fmt_f(r.y),
                            fmt_f(r.width),
                            fmt_f(r.height)
                        ));
                    }
                }
                out.push_str("ET\n");
            }
            "q" => {
                ctm_stack.push(ctm);
                out.push_str("q\n");
            }
            "Q" => {
                if let Some(prev) = ctm_stack.pop() {
                    ctm = prev;
                }
                out.push_str("Q\n");
            }
            "cm" => {
                if operands.len() == 6 {
                    let m = [
                        operands[0],
                        operands[1],
                        operands[2],
                        operands[3],
                        operands[4],
                        operands[5],
                    ];
                    ctm = matrix_multiply(&ctm, &m);
                }
                emit!("cm");
            }
            "Do" => {
                let name = pending_name.clone();
                let mut removed = false;
                if let Some(ref name) = name
                    && image_xobjects.get(name).is_some_and(|x| x.is_image)
                {
                    // `Do` maps the unit square through the CTM; the
                    // placement rect is that square's bounding box.
                    let corners = [
                        ctm_apply(&ctm, 0.0, 0.0),
                        ctm_apply(&ctm, 1.0, 0.0),
                        ctm_apply(&ctm, 0.0, 1.0),
                        ctm_apply(&ctm, 1.0, 1.0),
                    ];
                    let xs = [corners[0].0, corners[1].0, corners[2].0, corners[3].0];
                    let ys = [corners[0].1, corners[1].1, corners[2].1, corners[3].1];
                    let img_rect = Rect {
                        x: xs.iter().cloned().fold(f32::INFINITY, f32::min),
                        y: ys.iter().cloned().fold(f32::INFINITY, f32::min),
                        width: xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
                            - xs.iter().cloned().fold(f32::INFINITY, f32::min),
                        height: ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
                            - ys.iter().cloned().fold(f32::INFINITY, f32::min),
                    };
                    if regions.iter().any(|r| r.intersects(&img_rect)) {
                        removed_images.push(name.clone());
                        out.push_str("% redacted image\n");
                        removed = true;
                    }
                }
                if !removed {
                    emit!("Do");
                } else {
                    pending_name.take();
                }
            }
            "Tf" => {
                if !operands.is_empty() {
                    font_size = *operands.last().unwrap();
                }
                if let Some(name) = pending_name.as_ref() {
                    current_metrics = fonts.get(name).cloned();
                }
                emit!("Tf");
            }
            "Tm" => {
                if operands.len() == 6 {
                    let n = [
                        operands[0],
                        operands[1],
                        operands[2],
                        operands[3],
                        operands[4],
                        operands[5],
                    ];
                    text_matrix = n;
                    text_line_matrix = n;
                }
                emit!("Tm");
            }
            "Td" | "TD" => {
                if operands.len() == 2 {
                    let (tx, ty) = (operands[0], operands[1]);
                    let m = text_line_matrix;
                    text_line_matrix = [
                        m[0],
                        m[1],
                        m[2],
                        m[3],
                        m[0] * tx + m[2] * ty + m[4],
                        m[1] * tx + m[3] * ty + m[5],
                    ];
                    text_matrix = text_line_matrix;
                }
                emit!(op);
            }
            "T*" => {
                let m = text_line_matrix;
                let new_ey = m[5] - font_size;
                text_line_matrix = [m[0], m[1], m[2], m[3], m[4], new_ey];
                text_matrix = text_line_matrix;
                out.push_str("T*\n");
            }
            "Tj" => {
                if let Some(text) = extract_string(&tokens, i) {
                    // Mask text-showing operators even outside BT…ET:
                    // such operators are invalid PDF but must not leak.
                    let (x, y) = (text_matrix[4], text_matrix[5]);
                    let masked = mask_string_partial(
                        &text,
                        x,
                        y,
                        font_size,
                        current_metrics.as_ref(),
                        regions,
                    );
                    out.push('(');
                    out.push_str(&masked);
                    out.push_str(") Tj\n");
                    let width = text_width(&text, font_size, current_metrics.as_ref());
                    text_matrix[4] = x + width;
                }
            }
            "TJ" => {
                if let Some(items) = extract_tj_array(&tokens, i) {
                    let mut x = text_matrix[4];
                    let y = text_matrix[5];
                    out.push('[');
                    for item in items {
                        match item {
                            TjItem::Text(t) => {
                                let masked = mask_string_partial(
                                    &t,
                                    x,
                                    y,
                                    font_size,
                                    current_metrics.as_ref(),
                                    regions,
                                );
                                out.push('(');
                                out.push_str(&masked);
                                out.push(')');
                                let width = text_width(&t, font_size, current_metrics.as_ref());
                                x += width;
                            }
                            TjItem::Kern(amount) => {
                                out.push_str(&format!(" {} ", fmt_f(amount)));
                                x += amount;
                            }
                        }
                    }
                    out.push_str("] TJ\n");
                    text_matrix[4] = x;
                }
            }
            "'" => {
                if let Some(text) = extract_string(&tokens, i) {
                    let m = text_line_matrix;
                    let new_ey = m[5] - font_size;
                    text_line_matrix = [m[0], m[1], m[2], m[3], m[4], new_ey];
                    text_matrix = text_line_matrix;
                    out.push_str("T*\n(");
                    let (x, y) = (text_matrix[4], text_matrix[5]);
                    let masked = mask_string_partial(
                        &text,
                        x,
                        y,
                        font_size,
                        current_metrics.as_ref(),
                        regions,
                    );
                    out.push_str(&masked);
                    out.push_str(") Tj\n");
                }
            }
            _ => {
                // Pass through any token we don't explicitly handle, EXCEPT
                // literal PDF strings and hex strings — those are operands to
                // Tj/TJ which we already rewrite above.
                if op.starts_with('(') || op.starts_with('<') || op.starts_with('[') {
                    // Skip — handled by Tj/TJ arms.
                } else {
                    emit!(op);
                }
            }
        }
        pending_name = None;
        operands.clear();
        i += 1;
    }
    (out, removed_images)
}

/// Apply a 6-element affine matrix to a point.
fn ctm_apply(m: &[f32; 6], x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// Multiply two 2D affine matrices (6-element: a, b, c, d, e, f).
pub(super) fn matrix_multiply(a: &[f32; 6], b: &[f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

/// Mask only the characters whose individual bounding boxes intersect a redaction region.
/// Characters outside any region are preserved.
pub(super) fn mask_string_partial(
    text: &str,
    start_x: f32,
    y: f32,
    font_size: f32,
    metrics: Option<&crate::search::FontMetrics>,
    regions: &[Rect],
) -> String {
    let mut x = start_x;
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        let advance = metrics.map(|m| m.advance(ch as u32)).unwrap_or(500);
        let char_width = advance as f32 * font_size / 1000.0;
        let char_bbox = Rect {
            x,
            y: y - font_size,
            width: char_width,
            height: font_size,
        };
        if regions.iter().any(|r| r.intersects(&char_bbox)) {
            result.push(' ');
        } else {
            result.push(ch);
        }
        x += char_width;
    }
    result
}

fn fmt_f(v: f32) -> String {
    let s = format!("{:.4}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn text_width(text: &str, font_size: f32, metrics: Option<&crate::search::FontMetrics>) -> f32 {
    let mut units = 0u32;
    for ch in text.chars() {
        let advance = metrics.map(|m| m.advance(ch as u32)).unwrap_or(500);
        units += advance as u32;
    }
    units as f32 * font_size / 1000.0
}

// ----- Plumbing -----------------------------------------------------------

/// Re-use the search module's string extractor.
fn extract_string(tokens: &[String], i: usize) -> Option<String> {
    crate::search::extract_string(tokens, i)
}
