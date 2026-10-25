//! Content-stream parsing & dispatch: tokeniser and operator handling.

use std::collections::HashMap;

use super::clip::ClipMask;
use super::fonts::FontMetrics;
use super::forms::{self, FormData};
use super::images::{self, RasterImage};
use super::resources::{ExtGState, PageResources};
use super::surface::{Color, PathSegment, Surface};
use crate::pdf::PdfDocument;

mod text;
mod tokens;

use text::draw_text_bytes;
use tokens::{
    cmyk_to_rgb, extract_array_strings_raw, extract_font_name, extract_name_before,
    extract_string_raw, parse_number, tokenize,
};

/// Everything the content-stream interpreter needs beyond the surface.
pub(super) struct RenderContext<'a> {
    /// Page font metrics by resource name.
    pub fonts: &'a HashMap<String, FontMetrics>,
    /// Page resources (`/ExtGState`, `/XObject`).
    pub resources: &'a PageResources,
    /// Original PDF bytes, used to decode image XObjects and Form XObjects.
    pub pdf_bytes: &'a [u8],
    /// Parsed document, used to resolve Form XObject resources.
    pub doc: &'a PdfDocument,
    /// Cache of decoded images, keyed by XObject number.
    pub images: &'a mut HashMap<u32, Option<RasterImage>>,
    /// Cache of decoded Form XObjects.
    pub forms: &'a mut HashMap<u32, Option<FormData>>,
    /// Stack of enclosing Form XObjects' resources, innermost last.
    pub scopes: Vec<(HashMap<String, FontMetrics>, PageResources)>,
    /// Current Form XObject nesting depth.
    pub depth: usize,
}

impl RenderContext<'_> {
    /// Decode an image XObject, caching the result.
    fn image(&mut self, obj_id: u32) -> Option<RasterImage> {
        if let Some(cached) = self.images.get(&obj_id) {
            return cached.clone();
        }
        let decoded = images::decode_image_xobject(self.pdf_bytes, obj_id);
        self.images.insert(obj_id, decoded.clone());
        decoded
    }

    /// Resolve a font by resource name, innermost Form XObject first.
    fn font(&self, name: &str) -> Option<FontMetrics> {
        for (fonts, _) in self.scopes.iter().rev() {
            if let Some(metrics) = fonts.get(name) {
                return Some(metrics.clone());
            }
        }
        self.fonts.get(name).cloned()
    }

    /// Resolve an `/ExtGState` by resource name, innermost Form XObject first.
    fn ext_gstate(&self, name: &str) -> Option<&ExtGState> {
        for (_, resources) in self.scopes.iter().rev() {
            if let Some(gs) = resources.ext_gstate(name) {
                return Some(gs);
            }
        }
        self.resources.ext_gstate(name)
    }

    /// Resolve an XObject object number by resource name, innermost first.
    fn xobject(&self, name: &str) -> Option<u32> {
        for (_, resources) in self.scopes.iter().rev() {
            if let Some(id) = resources.xobjects.get(name) {
                return Some(*id);
            }
        }
        self.resources.xobjects.get(name).copied()
    }
}

/// The fill rule used when a path becomes a clipping path.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ClipRule {
    /// Non-zero winding (`W`).
    NonZero,
    /// Even-odd (`W*`).
    EvenOdd,
}

pub(super) fn render_content_stream(surface: &mut Surface, src: &str, ctx: &mut RenderContext<'_>) {
    let tokens = tokenize(src);
    let mut i = 0;
    let mut operands: Vec<f32> = Vec::new();
    // Set by `W` / `W*`; applied when the path is consumed.
    let mut pending_clip: Option<ClipRule> = None;
    while i < tokens.len() {
        let t = &tokens[i];
        if let Some(n) = parse_number(t) {
            operands.push(n);
            i += 1;
            continue;
        }
        let op = t.as_str();
        match op {
            "q" => {
                surface.state_stack.push(surface.state.clone());
            }
            "Q" => {
                if let Some(state) = surface.state_stack.pop() {
                    surface.state = state;
                }
            }
            "cm" => {
                if operands.len() == 6 {
                    let m = surface.transform;
                    let n = [
                        operands[0],
                        operands[1],
                        operands[2],
                        operands[3],
                        operands[4],
                        operands[5],
                    ];
                    surface.transform = [
                        m[0] * n[0] + m[2] * n[1],
                        m[1] * n[0] + m[3] * n[1],
                        m[0] * n[2] + m[2] * n[3],
                        m[1] * n[2] + m[3] * n[3],
                        m[0] * n[4] + m[2] * n[5] + m[4],
                        m[1] * n[4] + m[3] * n[5] + m[5],
                    ];
                }
            }
            "w" => {
                if let Some(v) = operands.first() {
                    surface.state.line_width = *v;
                }
            }
            "gs" => {
                if let Some(name) = extract_name_before(&tokens, i)
                    && let Some(gs) = ctx.ext_gstate(&name)
                {
                    if let Some(ca) = gs.fill_alpha {
                        surface.state.fill_alpha = ca.clamp(0.0, 1.0);
                    }
                    if let Some(ca) = gs.stroke_alpha {
                        surface.state.stroke_alpha = ca.clamp(0.0, 1.0);
                    }
                    if let Some(lw) = gs.line_width {
                        surface.state.line_width = lw;
                    }
                }
            }
            "Tr" => {
                if let Some(&mode) = operands.last() {
                    surface.state.text_render_mode = mode.clamp(0.0, 7.0) as u8;
                }
            }
            "W" => {
                pending_clip = Some(ClipRule::NonZero);
            }
            "W*" => {
                pending_clip = Some(ClipRule::EvenOdd);
            }
            "rg" => {
                if operands.len() == 3 {
                    surface.state.fill = Color::from_rgb(operands[0], operands[1], operands[2]);
                }
            }
            "RG" => {
                if operands.len() == 3 {
                    surface.state.stroke = Color::from_rgb(operands[0], operands[1], operands[2]);
                }
            }
            "g" => {
                if let Some(v) = operands.first() {
                    surface.state.fill = Color::from_gray(*v);
                }
            }
            "G" => {
                if let Some(v) = operands.first() {
                    surface.state.stroke = Color::from_gray(*v);
                }
            }
            "k" | "K" => {
                if operands.len() == 4 {
                    let c = cmyk_to_rgb(operands[0], operands[1], operands[2], operands[3]);
                    if op == "k" {
                        surface.state.fill = c;
                    } else {
                        surface.state.stroke = c;
                    }
                }
            }
            "m" => {
                if operands.len() >= 2 {
                    let x = operands[operands.len() - 2];
                    let y = operands[operands.len() - 1];
                    surface.path.push(PathSegment::Move(x, y));
                    surface.subpath_start = (x, y);
                    surface.current_point = (x, y);
                }
            }
            "l" => {
                if operands.len() >= 2 {
                    let x = operands[operands.len() - 2];
                    let y = operands[operands.len() - 1];
                    surface.path.push(PathSegment::Line(x, y));
                    surface.current_point = (x, y);
                }
            }
            "c" => {
                if operands.len() >= 6 {
                    let n = operands.len();
                    let x3 = operands[n - 2];
                    let y3 = operands[n - 1];
                    let x2 = operands[n - 4];
                    let y2 = operands[n - 3];
                    let x1 = operands[n - 6];
                    let y1 = operands[n - 5];
                    surface
                        .path
                        .push(PathSegment::Curve(x1, y1, x2, y2, x3, y3));
                    surface.current_point = (x3, y3);
                }
            }
            "re" => {
                if operands.len() >= 4 {
                    let n = operands.len();
                    let x = operands[n - 4];
                    let y = operands[n - 3];
                    let w = operands[n - 2];
                    let h = operands[n - 1];
                    surface.path.push(PathSegment::Move(x, y));
                    surface.path.push(PathSegment::Line(x + w, y));
                    surface.path.push(PathSegment::Line(x + w, y + h));
                    surface.path.push(PathSegment::Line(x, y + h));
                    surface.path.push(PathSegment::Close);
                    surface.subpath_start = (x, y);
                    surface.current_point = (x, y);
                }
            }
            "h" => {
                surface.path.push(PathSegment::Close);
                surface.current_point = surface.subpath_start;
            }
            "S" | "s" => {
                surface.stroke_path();
                apply_pending_clip(surface, &mut pending_clip);
                surface.path.clear();
            }
            "f" | "F" | "B" | "b" | "n" => {
                if op != "n" {
                    surface.fill_path();
                }
                if op == "B" || op == "b" {
                    surface.stroke_path();
                }
                apply_pending_clip(surface, &mut pending_clip);
                surface.path.clear();
            }
            "BT" => {
                surface.in_text = true;
                surface.text_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                surface.text_line_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
            }
            "ET" => {
                surface.in_text = false;
            }
            "Tf" => {
                // Only the size lands in `operands` (the /Name token is not
                // numeric); the name is recovered from the token stream.
                if let Some(&size) = operands.last()
                    && size > 0.0
                {
                    surface.font_size = size;
                }
                if i >= 2
                    && let Some(name) = extract_font_name(&tokens, i)
                {
                    surface.font_metrics = ctx.font(&name);
                }
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
                    surface.text_matrix = n;
                    surface.text_line_matrix = n;
                }
            }
            "Td" => {
                if operands.len() == 2 {
                    let (tx, ty) = (operands[0], operands[1]);
                    let m = surface.text_line_matrix;
                    surface.text_line_matrix = [
                        m[0],
                        m[1],
                        m[2],
                        m[3],
                        m[0] * tx + m[2] * ty + m[4],
                        m[1] * tx + m[3] * ty + m[5],
                    ];
                    surface.text_matrix = surface.text_line_matrix;
                }
            }
            "TD" => {
                if operands.len() == 2 {
                    let (tx, ty) = (operands[0], operands[1]);
                    let m = surface.text_line_matrix;
                    surface.text_line_matrix = [
                        m[0],
                        m[1],
                        m[2],
                        m[3],
                        m[0] * tx + m[2] * ty + m[4],
                        m[1] * tx + m[3] * ty + m[5],
                    ];
                    surface.text_matrix = surface.text_line_matrix;
                }
            }
            "T*" => {
                let m = surface.text_line_matrix;
                let new_ey = m[5] - surface.font_size;
                surface.text_line_matrix = [m[0], m[1], m[2], m[3], m[4], new_ey];
                surface.text_matrix = surface.text_line_matrix;
            }
            "Tj" => {
                if let Some(raw) = extract_string_raw(&tokens, i) {
                    draw_text_bytes(surface, &raw);
                }
            }
            "TJ" => {
                if let Some(raw) = extract_array_strings_raw(&tokens, i) {
                    draw_text_bytes(surface, &raw);
                }
            }
            "'" => {
                if let Some(raw) = extract_string_raw(&tokens, i) {
                    let m = surface.text_line_matrix;
                    let new_ey = m[5] - surface.font_size;
                    surface.text_line_matrix = [m[0], m[1], m[2], m[3], m[4], new_ey];
                    surface.text_matrix = surface.text_line_matrix;
                    draw_text_bytes(surface, &raw);
                }
            }
            "\"" => {
                // aw ac string
                if let Some(raw) = extract_string_raw(&tokens, i) {
                    draw_text_bytes(surface, &raw);
                }
            }
            "Do" => {
                if let Some(name) = extract_name_before(&tokens, i)
                    && let Some(obj_id) = ctx.xobject(&name)
                {
                    match images::object_subtype(ctx.pdf_bytes, obj_id).as_deref() {
                        Some("Image") => {
                            if let Some(image) = ctx.image(obj_id) {
                                images::draw_image(surface, &image);
                            }
                        }
                        Some("Form") => forms::render_form(surface, ctx, obj_id),
                        _ => {}
                    }
                }
            }
            _ => {
                // Unknown operator — skip
            }
        }
        operands.clear();
        i += 1;
    }
}

/// Install the clipping path recorded by `W` / `W*`, if any.
fn apply_pending_clip(surface: &mut Surface, pending: &mut Option<ClipRule>) {
    let Some(rule) = pending.take() else {
        return;
    };
    let polys = surface.path_device_polygons();
    if polys.is_empty() {
        return;
    }
    let mask = ClipMask::rasterize(
        &polys,
        rule == ClipRule::EvenOdd,
        surface.width,
        surface.height,
    );
    surface.set_clip(mask);
}
