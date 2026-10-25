//! Form XObject rendering.
//!
//! A Form XObject is a self-contained content stream with its own
//! `/Matrix`, `/BBox` and `/Resources`. Invoking it with `Do` renders its
//! content with the form matrix composed onto the current transformation
//! matrix, inheriting the caller's graphics state (colours, alpha, clip) but
//! bringing its own fonts and nested XObjects.
//!
//! Forms may nest (a form can invoke another form), so rendering is
//! depth-limited; the form's own graphics-state changes are rolled back when
//! it returns.

use super::clip::ClipMask;
use super::fonts::{FontMetrics, collect_font_metrics_with_raw};
use super::interpreter::{RenderContext, render_content_stream};
use super::raw::{RawValue, find_object_body, find_stream_span};
use super::resources::{PageResources, collect_page_resources, read_stream_dict};
use super::surface::Surface;

/// Maximum nesting depth for Form XObjects (a form invoking a form).
const MAX_FORM_DEPTH: usize = 8;

/// A decoded Form XObject, ready to render.
#[derive(Debug, Clone)]
pub(super) struct FormData {
    /// Decompressed content stream (text operators).
    pub content: String,
    /// `/Matrix` mapping form space to the invoking content stream's space.
    pub matrix: [f32; 6],
    /// Optional `/BBox` in form space; content is clipped to it.
    pub bbox: Option<[f32; 4]>,
    /// The form's own font resources (fall back to the page's when absent).
    pub fonts: std::collections::HashMap<String, FontMetrics>,
    /// The form's own `/Resources` (`/ExtGState`, `/XObject`).
    pub resources: PageResources,
}

/// Compose two PDF transformation matrices (`M1 × M2`, i.e. apply `M2` first).
fn mat_mul(m1: [f32; 6], m2: [f32; 6]) -> [f32; 6] {
    [
        m1[0] * m2[0] + m1[2] * m2[1],
        m1[1] * m2[0] + m1[3] * m2[1],
        m1[0] * m2[2] + m1[2] * m2[3],
        m1[1] * m2[2] + m1[3] * m2[3],
        m1[0] * m2[4] + m1[2] * m2[5] + m1[4],
        m1[1] * m2[4] + m1[3] * m2[5] + m1[5],
    ]
}

/// Read a numeric array of `N` entries from a raw dictionary.
fn read_number_array(dict: &[(String, RawValue)], key: &str) -> Option<Vec<f32>> {
    match dict.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
        Some(RawValue::Array(items)) => items
            .iter()
            .map(|v| v.as_number().map(|n| n as f32))
            .collect(),
        _ => None,
    }
}

/// Decode a Form XObject (content stream, matrix, bbox, resources).
pub(super) fn collect_form_data(ctx: &RenderContext<'_>, form_id: u32) -> Option<FormData> {
    let body = find_object_body(ctx.pdf_bytes, form_id)?;
    let dict = read_stream_dict(body);
    let matrix = read_number_array(&dict, "Matrix")
        .and_then(|v| <[f32; 6]>::try_from(v).ok())
        .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let bbox = read_number_array(&dict, "BBox").and_then(|v| <[f32; 4]>::try_from(v).ok());

    let (start, end) = find_stream_span(ctx.pdf_bytes, form_id)?;
    let decompressed = crate::search::decompress_stream(&ctx.pdf_bytes[start..end]);
    let content = String::from_utf8_lossy(&decompressed).into_owned();

    let fonts = collect_font_metrics_with_raw(ctx.doc, ctx.pdf_bytes, form_id);
    let resources = collect_page_resources(ctx.pdf_bytes, form_id);

    Some(FormData {
        content,
        matrix,
        bbox,
        fonts,
        resources,
    })
}

/// Render a Form XObject through the surface, depth-limited.
pub(super) fn render_form(surface: &mut Surface, ctx: &mut RenderContext<'_>, form_id: u32) {
    if ctx.depth >= MAX_FORM_DEPTH {
        return;
    }
    let cached = ctx.forms.get(&form_id).and_then(|entry| entry.clone());
    let data = match cached {
        Some(data) => data,
        None => {
            let decoded = collect_form_data(ctx, form_id);
            ctx.forms.insert(form_id, decoded.clone());
            match decoded {
                Some(data) => data,
                None => return,
            }
        }
    };

    // The form inherits the caller's graphics state but composes its matrix
    // onto the CTM, and its own state changes must not leak back out.
    let saved_transform = surface.transform;
    let saved_state = surface.state.clone();
    surface.transform = mat_mul(saved_transform, data.matrix);

    // Clip to the form's /BBox (in form space) mapped to device space.
    if let Some([x0, y0, x1, y1]) = data.bbox {
        let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        let device: Vec<(f32, f32)> = corners
            .iter()
            .map(|&(x, y)| surface.transform_pt(x, y))
            .collect();
        let mask = ClipMask::rasterize(&[device], false, surface.width, surface.height);
        surface.set_clip(mask);
    }

    ctx.scopes.push((data.fonts, data.resources));
    ctx.depth += 1;
    let content = data.content;
    render_content_stream(surface, &content, ctx);
    ctx.depth -= 1;
    ctx.scopes.pop();

    surface.transform = saved_transform;
    surface.state = saved_state;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_composition_applies_right_first() {
        // Translate then scale == scale then translate by the scaled amount.
        let translate = [1.0, 0.0, 0.0, 1.0, 10.0, 20.0];
        let scale = [2.0, 0.0, 0.0, 3.0, 0.0, 0.0];
        let m = mat_mul(translate, scale);
        // Point (1, 1): scale -> (2, 3); translate -> (12, 23).
        let x = m[0] * 1.0 + m[2] * 1.0 + m[4];
        let y = m[1] * 1.0 + m[3] * 1.0 + m[5];
        assert!((x - 12.0).abs() < 1e-4);
        assert!((y - 23.0).abs() < 1e-4);
    }

    #[test]
    fn matrix_identity_is_neutral() {
        let identity = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let m = [4.0, 1.0, 2.0, 5.0, 7.0, 9.0];
        assert_eq!(mat_mul(identity, m), m);
        assert_eq!(mat_mul(m, identity), m);
    }
}
