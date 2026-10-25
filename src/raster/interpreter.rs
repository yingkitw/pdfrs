//! Content-stream parsing & dispatch: tokeniser and operator handling.

use std::collections::HashMap;

use super::clip::ClipMask;
use super::fonts::FontMetrics;
use super::forms::{self, FormData};
use super::images::{self, RasterImage};
use super::resources::{ExtGState, PageResources};
use super::surface::{Color, GlyphOutlineBuilder, PathSegment, Surface, fill_polygons_raw};
use crate::pdf::PdfDocument;

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

/// Read a `/Name` operand that directly precedes the operator at `i`.
fn extract_name_before(tokens: &[String], i: usize) -> Option<String> {
    if i == 0 {
        return None;
    }
    tokens[i - 1].strip_prefix('/').map(|s| s.to_string())
}

fn extract_font_name(tokens: &[String], i: usize) -> Option<String> {
    if i < 2 {
        return None;
    }
    // `Tf` operands are `/Name size`; the name is two tokens before the operator.
    let prev = &tokens[i - 2];
    if let Some(stripped) = prev.strip_prefix('/') {
        return Some(stripped.to_string());
    }
    None
}

/// Extract raw bytes from the string operand preceding operator at index `i`.
/// For hex strings `<...>`, returns the decoded bytes. For literal strings `(...)`,
/// returns the decoded byte content.
fn extract_string_raw(tokens: &[String], i: usize) -> Option<Vec<u8>> {
    if i == 0 {
        return None;
    }
    let prev = &tokens[i - 1];
    let trimmed = prev.trim();
    if trimmed.starts_with('<') && trimmed.ends_with('>') {
        // Hex string
        let hex: String = trimmed[1..trimmed.len() - 1]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .filter_map(|j| {
                if j + 1 < hex.len() {
                    u8::from_str_radix(&hex[j..j + 2], 16).ok()
                } else if j < hex.len() {
                    u8::from_str_radix(&hex[j..j + 1], 16).ok()
                } else {
                    None
                }
            })
            .collect();
        Some(bytes)
    } else if trimmed.starts_with('(') && trimmed.ends_with(')') {
        // Literal string — decode escapes and return bytes
        let inner = &trimmed[1..trimmed.len() - 1];
        let bytes = inner.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut k = 0;
        while k < bytes.len() {
            let b = bytes[k];
            if b == b'\\' && k + 1 < bytes.len() {
                let nxt = bytes[k + 1];
                match nxt {
                    b'n' => {
                        out.push(b'\n');
                        k += 2;
                        continue;
                    }
                    b'r' => {
                        out.push(b'\r');
                        k += 2;
                        continue;
                    }
                    b't' => {
                        out.push(b'\t');
                        k += 2;
                        continue;
                    }
                    b'\\' | b'(' | b')' => {
                        out.push(nxt);
                        k += 2;
                        continue;
                    }
                    d if d.is_ascii_digit() => {
                        let mut oct = String::new();
                        oct.push(d as char);
                        let mut j = k + 2;
                        while j < bytes.len()
                            && oct.len() < 3
                            && (bytes[j] as char).is_ascii_digit()
                        {
                            oct.push(bytes[j] as char);
                            j += 1;
                        }
                        if let Ok(v) = u8::from_str_radix(&oct, 8) {
                            out.push(v);
                        }
                        k = j;
                        continue;
                    }
                    _ => {
                        out.push(b);
                        k += 1;
                        continue;
                    }
                }
            }
            out.push(b);
            k += 1;
        }
        Some(out)
    } else {
        None
    }
}

/// Extract raw bytes from a TJ array operand, concatenating all string elements
/// (both hex `<...>` and literal `(...)` strings) and skipping numeric kerning values.
fn extract_array_strings_raw(tokens: &[String], i: usize) -> Option<Vec<u8>> {
    if i == 0 {
        return None;
    }
    let prev = &tokens[i - 1];
    if !prev.starts_with('[') || !prev.ends_with(']') {
        return None;
    }
    let mut out = Vec::new();
    let bytes = prev.as_bytes();
    let mut k = 0;
    while k < bytes.len() {
        let c = bytes[k] as char;
        if c == '(' {
            let start = k + 1;
            let mut end = start;
            let mut d = 1;
            while end < bytes.len() && d > 0 {
                let cc = bytes[end] as char;
                if cc == '\\' && end + 1 < bytes.len() {
                    end += 2;
                    continue;
                }
                if cc == '(' {
                    d += 1;
                } else if cc == ')' {
                    d -= 1;
                }
                if d > 0 {
                    end += 1;
                }
            }
            // Decode literal string bytes
            let inner = &prev[start..end];
            if let Some(s) = parse_pdf_literal_string(&format!("({})", inner)) {
                out.extend_from_slice(s.as_bytes());
            }
            k = end + 1;
        } else if c == '<' {
            let start = k + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] as char != '>' {
                end += 1;
            }
            // Decode hex string bytes
            let hex: String = prev[start..end]
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect();
            for j in (0..hex.len()).step_by(2) {
                if j + 1 < hex.len() {
                    if let Ok(v) = u8::from_str_radix(&hex[j..j + 2], 16) {
                        out.push(v);
                    }
                } else if j < hex.len()
                    && let Ok(v) = u8::from_str_radix(&hex[j..j + 1], 16)
                {
                    out.push(v);
                }
            }
            k = end + 1;
        } else {
            k += 1;
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

fn parse_pdf_literal_string(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut k = 0;
    while k < bytes.len() {
        let b = bytes[k];
        if b == b'\\' && k + 1 < bytes.len() {
            let nxt = bytes[k + 1];
            match nxt {
                b'n' => {
                    out.push(b'\n');
                    k += 2;
                    continue;
                }
                b'r' => {
                    out.push(b'\r');
                    k += 2;
                    continue;
                }
                b't' => {
                    out.push(b'\t');
                    k += 2;
                    continue;
                }
                b'\\' | b'(' | b')' => {
                    out.push(nxt);
                    k += 2;
                    continue;
                }
                d if d.is_ascii_digit() => {
                    let mut oct = String::new();
                    oct.push(d as char);
                    let mut j = k + 2;
                    while j < bytes.len() && oct.len() < 3 && (bytes[j] as char).is_ascii_digit() {
                        oct.push(bytes[j] as char);
                        j += 1;
                    }
                    if let Ok(v) = u8::from_str_radix(&oct, 8) {
                        out.push(v);
                    }
                    k = j;
                    continue;
                }
                _ => {}
            }
        }
        // UTF-16BE BOM
        if out.is_empty() && b == 0xFE && k + 1 < bytes.len() && bytes[k + 1] == 0xFF {
            k += 2;
            while k + 1 < bytes.len() {
                let cu = u16::from_be_bytes([bytes[k], bytes[k + 1]]);
                if let Some(c) = char::from_u32(cu as u32) {
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    out.extend_from_slice(s.as_bytes());
                }
                k += 2;
            }
            return Some(String::from_utf8_lossy(&out).into_owned());
        }
        out.push(b);
        k += 1;
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// How a glyph is painted, derived from the text rendering mode (`Tr`).
#[derive(Debug, Clone, Copy, PartialEq)]
enum TextPaint {
    Fill,
    Stroke,
    FillAndStroke,
    Invisible,
}

/// Map a text rendering mode to its painting behaviour.
///
/// Modes 4–7 are the "clip" variants; clipping to the glyph outline is not
/// modelled, so they are treated as their non-clipping counterparts.
fn text_paint(mode: u8) -> TextPaint {
    match mode {
        0 | 4 => TextPaint::Fill,
        1 | 5 => TextPaint::Stroke,
        2 | 6 => TextPaint::FillAndStroke,
        _ => TextPaint::Invisible,
    }
}

/// Render text from raw content-stream bytes. For CIDFont (Type0) with embedded
/// TTF, bytes are interpreted as 2-byte glyph IDs. For simple fonts, bytes are
/// interpreted as ASCII/Latin-1 characters.
fn draw_text_bytes(surface: &mut Surface, raw: &[u8]) {
    if raw.is_empty() {
        return;
    }
    let size = surface.font_size;
    let paint = text_paint(surface.state.text_render_mode);
    let fill_color = surface.state.fill;
    let metrics = surface.font_metrics.clone();

    // If we have an embedded TTF, rasterise actual glyph outlines.
    if let Some(ttf_bytes) = metrics.as_ref().and_then(|m| m.embedded_ttf.as_ref())
        && let Ok(face) = ttf_parser::Face::parse(ttf_bytes, 0)
    {
        let upem = face.units_per_em().max(1) as f32;
        let scale = size / upem;
        let tm = surface.text_matrix;
        // Text-space cursor: baseline origin is (0,0); the text matrix maps
        // text space to user space, and transform_pt maps user → screen.
        let mut text_cursor: f32 = 0.0;

        // Determine if this is a 2-byte CIDFont (heuristic: even-length bytes
        // and font metrics has /W array with values > 255, or /Subtype /Type0).
        // For CIDFont, interpret bytes as 2-byte glyph IDs.
        // For simple fonts with embedded TTF, interpret as Unicode chars.
        let is_cid =
            raw.len().is_multiple_of(2) && raw.iter().filter(|&&b| b == 0).count() >= raw.len() / 4;

        if is_cid {
            // 2-byte glyph IDs
            let mut idx = 0;
            while idx + 1 < raw.len() {
                let gid = u16::from_be_bytes([raw[idx], raw[idx + 1]]);
                let gid_u32 = gid as u32;
                let advance_units = metrics.as_ref().map(|m| m.advance(gid_u32)).unwrap_or(500);
                let advance_pt = advance_units as f32 * size / 1000.0;

                if gid > 0 {
                    let mut builder = GlyphOutlineBuilder::new();
                    let _ = face.outline_glyph(ttf_parser::GlyphId(gid), &mut builder);
                    // Fit the outline to the advance width the PDF declares, so
                    // a mismatch between the font program's own metrics and the
                    // PDF width table cannot make glyphs overlap.
                    let x_scale =
                        horizontal_fit(&face, ttf_parser::GlyphId(gid), advance_units, upem);
                    let contours = screen_contours(
                        surface,
                        &builder.contours,
                        text_cursor,
                        scale,
                        x_scale,
                        &tm,
                    );
                    paint_glyph(surface, &contours, paint, fill_color);
                }
                text_cursor += advance_pt;
                idx += 2;
            }
        } else {
            // Simple font: interpret bytes as characters
            for &b in raw {
                let ch = b as char;
                let cp = ch as u32;
                let advance_units = metrics.as_ref().map(|m| m.advance(cp)).unwrap_or(500);
                let advance_pt = advance_units as f32 * size / 1000.0;

                if let Some(gid) = face.glyph_index(ch) {
                    let mut builder = GlyphOutlineBuilder::new();
                    let _ = face.outline_glyph(gid, &mut builder);
                    let x_scale = horizontal_fit(&face, gid, advance_units, upem);
                    let contours = screen_contours(
                        surface,
                        &builder.contours,
                        text_cursor,
                        scale,
                        x_scale,
                        &tm,
                    );
                    paint_glyph(surface, &contours, paint, fill_color);
                } else {
                    let ux = tm[0] * text_cursor + tm[4];
                    let uy = tm[1] * text_cursor + tm[5];
                    draw_glyph_rect(surface, ux, uy, advance_pt, size, paint, fill_color);
                }
                text_cursor += advance_pt;
            }
        }

        let m = surface.text_matrix;
        surface.text_matrix = [m[0], m[1], m[2], m[3], m[4] + text_cursor, m[5]];
        surface.text_line_matrix = surface.text_matrix;
        return;
    }

    // Fallback: gray glyph-block rectangles (base-14 or no embedded font).
    // Interpret bytes as characters for width lookup.
    let mut cursor_x = surface.text_matrix[4];
    let baseline_y = surface.text_matrix[5];
    for &b in raw {
        let cp = b as u32;
        let advance_units = metrics.as_ref().map(|m| m.advance(cp)).unwrap_or(500);
        let advance_pt = advance_units as f32 * size / 1000.0;
        draw_glyph_rect(
            surface, cursor_x, baseline_y, advance_pt, size, paint, fill_color,
        );
        cursor_x += advance_pt;
    }

    let total = cursor_x - surface.text_matrix[4];
    let m = surface.text_matrix;
    surface.text_matrix = [m[0], m[1], m[2], m[3], m[4] + total, m[5]];
    surface.text_line_matrix = surface.text_matrix;
}

/// Horizontal scale factor that makes a glyph occupy exactly the advance width
/// the PDF declares for it.
///
/// The font program's own metrics and the PDF's width table can disagree (for
/// example after font subsetting, or when a substitute font stands in for a
/// base-14 face). Drawing the outline at its natural width then lets it
/// overlap the neighbouring glyph. Scaling the outline — and only the
/// outline — keeps the PDF's advance-based layout while removing the overlap.
fn horizontal_fit(
    face: &ttf_parser::Face<'_>,
    gid: ttf_parser::GlyphId,
    advance_units: u16,
    upem: f32,
) -> f32 {
    let natural = face.glyph_hor_advance(gid).unwrap_or(0) as f32 * 1000.0 / upem;
    if natural <= 0.0 {
        return 1.0;
    }
    let target = advance_units as f32;
    // Guard against pathological tables producing absurd distortion.
    (target / natural).clamp(0.1, 10.0)
}

/// Map a glyph's font-space contours into device space.
fn screen_contours(
    surface: &Surface,
    contours: &[Vec<(f32, f32)>],
    text_cursor: f32,
    scale: f32,
    x_scale: f32,
    tm: &[f32; 6],
) -> Vec<Vec<(f32, f32)>> {
    contours
        .iter()
        .filter(|contour| contour.len() >= 3)
        .map(|contour| {
            contour
                .iter()
                .map(|(gx, gy)| {
                    let px = text_cursor + gx * scale * x_scale;
                    let py = gy * scale;
                    let tx = tm[0] * px + tm[2] * py + tm[4];
                    let ty = tm[1] * px + tm[3] * py + tm[5];
                    surface.transform_pt(tx, ty)
                })
                .collect()
        })
        .collect()
}

/// Paint one glyph according to the text rendering mode.
///
/// All contours of the glyph are filled together so that counters (the hole
/// in an "o") stay hollow: TrueType outlines express a counter as a second
/// contour wound in the opposite direction, which only the non-zero winding
/// rule accounts for.
fn paint_glyph(
    surface: &mut Surface,
    contours: &[Vec<(f32, f32)>],
    paint: TextPaint,
    fill_color: Color,
) {
    match paint {
        TextPaint::Invisible => {}
        TextPaint::Fill => fill_polygons_raw(surface, contours, fill_color),
        TextPaint::Stroke => {
            for contour in contours {
                surface.stroke_device_polyline(contour, true);
            }
        }
        TextPaint::FillAndStroke => {
            fill_polygons_raw(surface, contours, fill_color);
            for contour in contours {
                surface.stroke_device_polyline(contour, true);
            }
        }
    }
}

/// Draw a gray fallback rectangle for a glyph cell.
fn draw_glyph_rect(
    surface: &mut Surface,
    cursor_x: f32,
    baseline_y: f32,
    advance_pt: f32,
    size: f32,
    paint: TextPaint,
    fill_color: Color,
) {
    if paint == TextPaint::Invisible {
        return;
    }
    let cap_height = size * 0.7;
    let x0 = cursor_x;
    let y0 = baseline_y - cap_height * 0.2;
    let x1 = cursor_x + advance_pt * 0.92;
    let y1 = baseline_y + cap_height * 0.2;
    if paint == TextPaint::Stroke {
        let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        let device: Vec<(f32, f32)> = corners
            .iter()
            .map(|&(x, y)| surface.transform_pt(x, y))
            .collect();
        surface.stroke_device_polyline(&device, true);
        return;
    }
    let (sx0, sy0) = surface.transform_pt(x0, y0);
    let (sx1, sy1) = surface.transform_pt(x1, y1);
    let min_x = sx0.min(sx1).round() as i32;
    let max_x = sx0.max(sx1).round() as i32;
    let min_y = sy0.min(sy1).round() as i32;
    let max_y = sy0.max(sy1).round() as i32;
    let alpha = 0.55 * surface.state.fill_alpha;
    for py in min_y..=max_y {
        for px in min_x..=max_x {
            if px >= 0 && py >= 0 {
                surface.set_pixel(px as u32, py as u32, fill_color, alpha);
            }
        }
    }
}

fn parse_number(s: &str) -> Option<f32> {
    s.parse::<f32>().ok()
}

fn cmyk_to_rgb(c: f32, m: f32, y: f32, k: f32) -> Color {
    let r = (1.0 - c) * (1.0 - k);
    let g = (1.0 - m) * (1.0 - k);
    let b = (1.0 - y) * (1.0 - k);
    Color::from_rgb(r, g, b)
}

// ----- Tokenizer ----------------------------------------------------------

fn tokenize(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '%' => {
                while i < bytes.len() && (bytes[i] as char) != '\n' {
                    i += 1;
                }
            }
            '(' => {
                let start = i;
                let mut depth = 1;
                i += 1;
                while i < bytes.len() && depth > 0 {
                    let cc = bytes[i] as char;
                    if cc == '\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if cc == '(' {
                        depth += 1;
                    } else if cc == ')' {
                        depth -= 1;
                    }
                    i += 1;
                }
                out.push(src[start..i].to_string());
            }
            '<' => {
                // Could be hex string << dict or <...>
                if i + 1 < bytes.len() && bytes[i + 1] == b'<' {
                    out.push("<<".to_string());
                    i += 2;
                } else {
                    let start = i;
                    i += 1;
                    while i < bytes.len() && (bytes[i] as char) != '>' {
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1;
                    }
                    out.push(src[start..i].to_string());
                }
            }
            '>' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                    out.push(">>".to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            '[' => {
                // Arrays are typically emitted on a single line; capture until matching ']'.
                let start = i;
                let mut depth = 1;
                i += 1;
                while i < bytes.len() && depth > 0 {
                    let cc = bytes[i] as char;
                    if cc == '\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if cc == '[' {
                        depth += 1;
                    } else if cc == ']' {
                        depth -= 1;
                    }
                    i += 1;
                }
                out.push(src[start..i].to_string());
            }
            ']' => {
                i += 1;
            }
            '/' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    let cc = bytes[i] as char;
                    if cc.is_whitespace() || "/<>()[]{}".contains(cc) {
                        break;
                    }
                    i += 1;
                }
                out.push(src[start..i].to_string());
            }
            _ if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' => {
                let start = i;
                if c == '-' || c == '+' {
                    i += 1;
                }
                while i < bytes.len() {
                    let cc = bytes[i] as char;
                    if cc.is_ascii_digit() || cc == '.' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push(src[start..i].to_string());
            }
            _ if c.is_ascii_alphabetic() => {
                let start = i;
                while i < bytes.len() {
                    let cc = bytes[i] as char;
                    if cc.is_ascii_alphabetic() || cc == '*' || cc == '\'' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push(src[start..i].to_string());
            }
            _ => {
                i += 1;
            }
        }
    }
    out
}
