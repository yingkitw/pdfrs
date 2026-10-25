//! Text showing and glyph outline painting for the content-stream interpreter.

use crate::raster::surface::{Color, GlyphOutlineBuilder, Surface, fill_polygons_raw};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum TextPaint {
    Fill,
    Stroke,
    FillAndStroke,
    Invisible,
}

/// Map a text rendering mode to its painting behaviour.
///
/// Modes 4–7 are the "clip" variants; clipping to the glyph outline is not
/// modelled, so they are treated as their non-clipping counterparts.
pub(super) fn text_paint(mode: u8) -> TextPaint {
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
pub(super) fn draw_text_bytes(surface: &mut Surface, raw: &[u8]) {
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
pub(super) fn horizontal_fit(
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
pub(super) fn screen_contours(
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
pub(super) fn paint_glyph(
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
pub(super) fn draw_glyph_rect(
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
