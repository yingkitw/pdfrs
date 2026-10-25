//! Image XObject decoding and drawing.
//!
//! Decodes the stream of an image XObject into RGBA samples and paints it
//! into the rasterizer surface. Supported filters are `FlateDecode` (with
//! PNG/TIFF predictors), `DCTDecode` (baseline JPEG), `ASCIIHexDecode`,
//! `ASCII85Decode` and `RunLengthDecode`; unfiltered streams are read
//! directly. Colour spaces are DeviceGray, DeviceRGB, DeviceCMYK, Indexed and
//! ICCBased (by component count), plus `/ImageMask` stencils.

use super::jpeg::decode_jpeg;
use super::raw::{RawValue, find_object_body, find_stream_span};
use super::resources::{
    dict_name, dict_number, read_decode_parms, read_filters, read_stream_dict, value_after_key,
};
use super::surface::Surface;

/// A decoded image ready for painting.
#[derive(Debug, Clone)]
pub(super) struct RasterImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major RGBA samples (`width * height * 4` bytes).
    pub rgba: Vec<u8>,
    /// True for a stencil (`/ImageMask`) image: only the alpha channel is
    /// meaningful and the paint colour comes from the graphics state.
    pub is_mask: bool,
}

/// The colour space of an image.
#[derive(Debug, Clone)]
enum ColorSpace {
    Gray,
    Rgb,
    Cmyk,
    Indexed {
        palette: Vec<u8>,
    },
    /// Unknown colour space with a known component count.
    Components(usize),
}

impl ColorSpace {
    fn components(&self) -> usize {
        match self {
            ColorSpace::Gray => 1,
            ColorSpace::Rgb => 3,
            ColorSpace::Cmyk => 4,
            ColorSpace::Indexed { .. } => 1,
            ColorSpace::Components(n) => *n,
        }
    }
}

/// Decode the image XObject with the given object number.
pub(super) fn decode_image_xobject(pdf_bytes: &[u8], obj_id: u32) -> Option<RasterImage> {
    let body = find_object_body(pdf_bytes, obj_id)?;
    let dict = read_stream_dict(body);
    let width = dict_number(&dict, "Width")? as u32;
    let height = dict_number(&dict, "Height")? as u32;
    if width == 0 || height == 0 || width > 20_000 || height > 20_000 {
        return None;
    }
    let bits = dict_number(&dict, "BitsPerComponent").unwrap_or(8.0) as usize;
    if !matches!(bits, 1 | 2 | 4 | 8 | 16) {
        return None;
    }
    let is_mask = dict
        .iter()
        .find(|(k, _)| k == "ImageMask")
        .and_then(|(_, v)| match v {
            RawValue::Name(name) => Some(name == "true"),
            _ => None,
        })
        .unwrap_or(false);

    let colorspace = dict
        .iter()
        .find(|(k, _)| k == "ColorSpace")
        .map(|(_, v)| parse_colorspace(v, pdf_bytes))
        .unwrap_or(ColorSpace::Gray);

    let filters = read_filters(&dict);
    let (start, end) = find_stream_span(pdf_bytes, obj_id)?;
    let raw = &pdf_bytes[start..end];

    // A DCTDecode payload is a complete JPEG: it is decoded to pixels
    // directly rather than to raw samples.
    if filters.iter().any(|f| f == "DCTDecode" || f == "DCT") {
        let jpeg = decode_jpeg(raw)?;
        let mut image = RasterImage {
            width: jpeg.width,
            height: jpeg.height,
            rgba: jpeg.rgba,
            is_mask: false,
        };
        apply_soft_mask(pdf_bytes, &dict, &mut image);
        return Some(image);
    }

    let decoded = decode_filters(raw, &filters)?;
    let decoded = apply_predictor(
        &decoded,
        read_decode_parms(&dict, pdf_bytes),
        bits,
        width as usize,
    )?;

    let decode = read_decode_array(&dict);
    let mut rgba = vec![255u8; width as usize * height as usize * 4];

    if is_mask {
        // Stencil: a 1-bit image where the sample selects "do not paint".
        let inverted = decode
            .as_ref()
            .and_then(|d| d.first().map(|min| *min > 0.5))
            .unwrap_or(false);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let row_start = y * row_bytes(width as usize, 1, bits);
                let bit = read_sample(&decoded, row_start, x, bits);
                let paint = if inverted { bit != 0 } else { bit == 0 };
                let i = (y * width as usize + x) * 4;
                rgba[i] = 0;
                rgba[i + 1] = 0;
                rgba[i + 2] = 0;
                rgba[i + 3] = if paint { 255 } else { 0 };
            }
        }
        return Some(RasterImage {
            width,
            height,
            rgba,
            is_mask: true,
        });
    }

    let ncomp = colorspace.components();
    for y in 0..height as usize {
        for x in 0..width as usize {
            let row_start = y * row_bytes(width as usize, ncomp, bits);
            let mut components = [0u8; 4];
            for (c, component) in components.iter_mut().enumerate().take(ncomp.min(4)) {
                let raw_value = read_sample(&decoded, row_start, x * ncomp + c, bits);
                *component = apply_decode(raw_value, bits, decode.as_deref(), c);
            }
            let (r, g, b) = to_rgb(&colorspace, &components);
            let i = (y * width as usize + x) * 4;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 255;
        }
    }

    // Soft mask: an alpha channel stored in a separate grayscale image.
    let mut image = RasterImage {
        width,
        height,
        rgba,
        is_mask: false,
    };
    apply_soft_mask(pdf_bytes, &dict, &mut image);
    Some(image)
}

/// Apply a `/SMask` soft mask to an image's alpha channel.
fn apply_soft_mask(pdf_bytes: &[u8], dict: &[(String, RawValue)], image: &mut RasterImage) {
    let Some(smask_id) = dict
        .iter()
        .find(|(k, _)| k == "SMask")
        .and_then(|(_, v)| v.as_reference())
    else {
        return;
    };
    let Some(mask) = decode_image_xobject(pdf_bytes, smask_id) else {
        return;
    };
    for y in 0..image.height as usize {
        for x in 0..image.width as usize {
            let mi = (y * mask.width as usize + x) * 4;
            let alpha = if mi < mask.rgba.len() {
                mask.rgba[mi]
            } else {
                255
            };
            let i = (y * image.width as usize + x) * 4;
            if i + 3 < image.rgba.len() {
                image.rgba[i + 3] = alpha;
            }
        }
    }
}

/// Bytes per image row.
fn row_bytes(width: usize, components: usize, bits: usize) -> usize {
    (width * components * bits).div_ceil(8)
}

/// Read one sample value from packed row data.
fn read_sample(data: &[u8], row_start: usize, index: usize, bits: usize) -> u8 {
    let bit = index * bits;
    let byte = row_start + bit / 8;
    match bits {
        16 => data.get(byte).copied().unwrap_or(0),
        8 => data.get(byte).copied().unwrap_or(0),
        _ => {
            let shift = 8 - bits - (bit % 8);
            let mask = (1u16 << bits) - 1;
            match data.get(byte) {
                Some(value) => (((*value as u16) >> shift) & mask) as u8,
                None => 0,
            }
        }
    }
}

/// Apply the `/Decode` array to a raw sample value.
fn apply_decode(value: u8, bits: usize, decode: Option<&[f32]>, component: usize) -> u8 {
    let Some(decode) = decode else {
        return value;
    };
    if decode.len() < (component + 1) * 2 {
        return value;
    }
    let min = decode[component * 2];
    let max = decode[component * 2 + 1];
    let max_raw = ((1u32 << bits.min(16)) - 1) as f32;
    if max_raw <= 0.0 {
        return value;
    }
    let normalized = value as f32 / max_raw;
    let mapped = min + normalized * (max - min);
    (mapped.clamp(0.0, 1.0) * max_raw).round() as u8
}

/// Convert component samples to RGB.
fn to_rgb(colorspace: &ColorSpace, components: &[u8; 4]) -> (u8, u8, u8) {
    match colorspace {
        ColorSpace::Gray => (components[0], components[0], components[0]),
        ColorSpace::Rgb => (components[0], components[1], components[2]),
        ColorSpace::Cmyk => {
            let (c, m, y, k) = (
                components[0] as f32 / 255.0,
                components[1] as f32 / 255.0,
                components[2] as f32 / 255.0,
                components[3] as f32 / 255.0,
            );
            (
                (255.0 * (1.0 - c) * (1.0 - k)).clamp(0.0, 255.0) as u8,
                (255.0 * (1.0 - m) * (1.0 - k)).clamp(0.0, 255.0) as u8,
                (255.0 * (1.0 - y) * (1.0 - k)).clamp(0.0, 255.0) as u8,
            )
        }
        ColorSpace::Indexed { palette } => {
            let index = components[0] as usize * 3;
            if index + 2 < palette.len() {
                (palette[index], palette[index + 1], palette[index + 2])
            } else {
                (0, 0, 0)
            }
        }
        ColorSpace::Components(n) => match n {
            1 => (components[0], components[0], components[0]),
            3 => (components[0], components[1], components[2]),
            4 => {
                let (c, m, y, k) = (
                    components[0] as f32 / 255.0,
                    components[1] as f32 / 255.0,
                    components[2] as f32 / 255.0,
                    components[3] as f32 / 255.0,
                );
                (
                    (255.0 * (1.0 - c) * (1.0 - k)).clamp(0.0, 255.0) as u8,
                    (255.0 * (1.0 - m) * (1.0 - k)).clamp(0.0, 255.0) as u8,
                    (255.0 * (1.0 - y) * (1.0 - k)).clamp(0.0, 255.0) as u8,
                )
            }
            _ => (components[0], components[0], components[0]),
        },
    }
}

/// Read the `/Decode` array of an image dictionary.
fn read_decode_array(dict: &[(String, RawValue)]) -> Option<Vec<f32>> {
    let value = dict.iter().find(|(k, _)| k == "Decode").map(|(_, v)| v)?;
    match value {
        RawValue::Array(items) => Some(
            items
                .iter()
                .map(|v| v.as_number().unwrap_or(0.0) as f32)
                .collect(),
        ),
        _ => None,
    }
}

/// Parse an image `/ColorSpace` value.
fn parse_colorspace(value: &RawValue, pdf_bytes: &[u8]) -> ColorSpace {
    match value {
        RawValue::Name(name) => match name.as_str() {
            "DeviceGray" | "G" | "CalGray" => ColorSpace::Gray,
            "DeviceRGB" | "RGB" | "CalRGB" => ColorSpace::Rgb,
            "DeviceCMYK" | "CMYK" => ColorSpace::Cmyk,
            _ => ColorSpace::Components(1),
        },
        RawValue::Array(items) => {
            let Some(RawValue::Name(kind)) = items.first() else {
                return ColorSpace::Components(1);
            };
            match kind.as_str() {
                "Indexed" | "I" => {
                    // [/Indexed base hival lookup]
                    let palette = match items.get(3) {
                        Some(RawValue::Bytes(bytes)) => bytes.clone(),
                        _ => Vec::new(),
                    };
                    ColorSpace::Indexed { palette }
                }
                "ICCBased" => {
                    let components = items
                        .get(1)
                        .and_then(|v| v.as_reference())
                        .and_then(|id| {
                            let body = find_object_body(pdf_bytes, id)?;
                            let dict = read_stream_dict(body);
                            dict_number(&dict, "N").map(|n| n as usize)
                        })
                        .unwrap_or(3);
                    ColorSpace::Components(components)
                }
                "DeviceN" => {
                    let components = items
                        .get(1)
                        .map(|v| match v {
                            RawValue::Array(names) => names.len(),
                            _ => 1,
                        })
                        .unwrap_or(1);
                    ColorSpace::Components(components)
                }
                "DeviceRGB" | "RGB" => ColorSpace::Rgb,
                "DeviceGray" | "G" => ColorSpace::Gray,
                "DeviceCMYK" | "CMYK" => ColorSpace::Cmyk,
                _ => ColorSpace::Components(1),
            }
        }
        _ => ColorSpace::Components(1),
    }
}

/// Apply the stream filters in order.
fn decode_filters(raw: &[u8], filters: &[String]) -> Option<Vec<u8>> {
    let mut data = raw.to_vec();
    for filter in filters {
        data = match filter.as_str() {
            "FlateDecode" | "Fl" => crate::compression::decompress_deflate(&data).ok()?,
            "DCTDecode" | "DCT" => {
                // Handled by the caller: a JPEG payload decodes to pixels, not
                // to raw samples.
                return None;
            }
            "ASCIIHexDecode" | "AHx" => ascii_hex_decode(&data),
            "ASCII85Decode" | "A85" => ascii85_decode(&data),
            "RunLengthDecode" | "RL" => run_length_decode(&data),
            _ => return None,
        };
    }
    Some(data)
}

/// Decode `ASCIIHexDecode` data.
fn ascii_hex_decode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut high: Option<u8> = None;
    for &byte in data {
        if byte == b'>' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        let Some(value) = (byte as char).to_digit(16) else {
            continue;
        };
        match high {
            None => high = Some(value as u8),
            Some(h) => {
                out.push((h << 4) | value as u8);
                high = None;
            }
        }
    }
    if let Some(h) = high {
        out.push(h << 4);
    }
    out
}

/// Decode `ASCII85Decode` data.
fn ascii85_decode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut group = [0u8; 5];
    let mut count = 0usize;
    let mut i = 0usize;
    while i < data.len() {
        let byte = data[i];
        i += 1;
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'~' {
            break;
        }
        if byte == b'z' && count == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        if !(b'!'..=b'u').contains(&byte) {
            continue;
        }
        group[count] = byte - b'!';
        count += 1;
        if count == 5 {
            let mut value = 0u32;
            for &g in &group {
                value = value.wrapping_mul(85).wrapping_add(g as u32);
            }
            out.extend_from_slice(&value.to_be_bytes());
            count = 0;
        }
    }
    if count > 0 {
        let missing = 5 - count;
        let mut padded = group;
        for slot in padded.iter_mut().skip(count) {
            *slot = 84;
        }
        let mut value = 0u32;
        for &g in &padded {
            value = value.wrapping_mul(85).wrapping_add(g as u32);
        }
        let bytes = value.to_be_bytes();
        out.extend_from_slice(&bytes[..4 - missing]);
    }
    out
}

/// Decode `RunLengthDecode` data.
fn run_length_decode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        let length = data[i];
        i += 1;
        if length == 128 {
            break;
        }
        if length < 128 {
            let count = length as usize + 1;
            let end = (i + count).min(data.len());
            out.extend_from_slice(&data[i..end]);
            i = end;
        } else {
            if i >= data.len() {
                break;
            }
            let byte = data[i];
            i += 1;
            let count = 257 - length as usize;
            out.extend(std::iter::repeat_n(byte, count));
        }
    }
    out
}

/// Undo a stream predictor (`/DecodeParms /Predictor`).
fn apply_predictor(
    data: &[u8],
    parms: Option<(u32, usize, usize, usize)>,
    bits: usize,
    width: usize,
) -> Option<Vec<u8>> {
    let Some((predictor, colors, bits_per_component, columns)) = parms else {
        return Some(data.to_vec());
    };
    if predictor <= 1 {
        return Some(data.to_vec());
    }
    let colors = colors.max(1);
    let bpc = if bits_per_component == 0 {
        bits
    } else {
        bits_per_component
    };
    if predictor == 2 {
        // TIFF predictor: horizontal differencing.
        let mut out = data.to_vec();
        let row_len = width * colors;
        if bpc == 8 {
            for row in out.chunks_mut(row_len) {
                for i in colors..row.len() {
                    row[i] = row[i].wrapping_add(row[i - colors]);
                }
            }
        }
        return Some(out);
    }
    // PNG predictors (10..=15): each row is prefixed with a filter byte.
    let row_bytes = (columns * colors * bpc).div_ceil(8);
    let bpp = ((colors * bpc).div_ceil(8)).max(1);
    let mut out = Vec::with_capacity(data.len());
    let mut previous = vec![0u8; row_bytes];
    let mut offset = 0usize;
    while offset + 1 + row_bytes <= data.len() + row_bytes && offset < data.len() {
        let filter = data[offset];
        offset += 1;
        if offset + row_bytes > data.len() {
            break;
        }
        let mut row = data[offset..offset + row_bytes].to_vec();
        offset += row_bytes;
        match filter {
            0 => {}
            1 => {
                for i in bpp..row.len() {
                    row[i] = row[i].wrapping_add(row[i - bpp]);
                }
            }
            2 => {
                for i in 0..row.len() {
                    row[i] = row[i].wrapping_add(previous[i]);
                }
            }
            3 => {
                for i in 0..row.len() {
                    let left = if i >= bpp { row[i - bpp] as u16 } else { 0 };
                    let up = previous[i] as u16;
                    row[i] = row[i].wrapping_add(((left + up) / 2) as u8);
                }
            }
            4 => {
                for i in 0..row.len() {
                    let a = if i >= bpp { row[i - bpp] as i16 } else { 0 };
                    let b = previous[i] as i16;
                    let c = if i >= bpp {
                        previous[i - bpp] as i16
                    } else {
                        0
                    };
                    let p = a + b - c;
                    let pa = (p - a).abs();
                    let pb = (p - b).abs();
                    let pc = (p - c).abs();
                    let predictor_value = if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    };
                    row[i] = row[i].wrapping_add(predictor_value as u8);
                }
            }
            _ => return None,
        }
        out.extend_from_slice(&row);
        previous = row;
    }
    Some(out)
}

/// Paint a decoded image through the current transformation matrix.
///
/// The image occupies the unit square, mapped by the CTM, exactly as PDF
/// defines image XObject placement.
pub(super) fn draw_image(surface: &mut Surface, image: &RasterImage) {
    if image.width == 0 || image.height == 0 {
        return;
    }
    let m = surface.transform;
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < f32::EPSILON {
        return;
    }
    // Inverse of the CTM, used to map device pixels back into the unit square.
    let (ia, ib, ic, id) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    let (ie, if_) = (
        (m[2] * m[5] - m[3] * m[4]) / det,
        (m[1] * m[4] - m[0] * m[5]) / det,
    );

    // Device-space bounding box of the unit square.
    let corners = [(0.0f32, 0.0f32), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (cx, cy) in corners {
        let sx = m[0] * cx + m[2] * cy + m[4];
        let sy = m[1] * cx + m[3] * cy + m[5];
        min_x = min_x.min(sx);
        min_y = min_y.min(sy);
        max_x = max_x.max(sx);
        max_y = max_y.max(sy);
    }
    let x0 = (min_x.floor() as i32).max(0);
    let y0 = (min_y.floor() as i32).max(0);
    let x1 = (max_x.ceil() as i32 + 1).min(surface.width as i32);
    let y1 = (max_y.ceil() as i32 + 1).min(surface.height as i32);

    let fill = surface.state.fill;
    let fill_alpha = surface.state.fill_alpha;

    for py in y0..y1 {
        for px in x0..x1 {
            let dx = px as f32 + 0.5;
            let dy = py as f32 + 0.5;
            let ux = ia * dx + ic * dy + ie;
            let uy = ib * dx + id * dy + if_;
            if !(0.0..=1.0).contains(&ux) || !(0.0..=1.0).contains(&uy) {
                continue;
            }
            let sx = ((ux * image.width as f32) as usize).min(image.width as usize - 1);
            let sy = (((1.0 - uy) * image.height as f32) as usize).min(image.height as usize - 1);
            let index = (sy * image.width as usize + sx) * 4;
            if index + 3 >= image.rgba.len() {
                continue;
            }
            if image.is_mask {
                let alpha = image.rgba[index + 3];
                if alpha == 0 {
                    continue;
                }
                surface.set_pixel(px as u32, py as u32, fill, fill_alpha);
            } else {
                let alpha = image.rgba[index + 3] as f32 / 255.0;
                if alpha <= 0.0 {
                    continue;
                }
                let color = super::surface::Color::from_rgb(
                    image.rgba[index] as f32 / 255.0,
                    image.rgba[index + 1] as f32 / 255.0,
                    image.rgba[index + 2] as f32 / 255.0,
                );
                surface.set_pixel(px as u32, py as u32, color, alpha * fill_alpha);
            }
        }
    }
}

/// Read the `/Subtype` of an object, e.g. `Image` or `Form`.
pub(super) fn object_subtype(pdf_bytes: &[u8], obj_id: u32) -> Option<String> {
    let body = find_object_body(pdf_bytes, obj_id)?;
    let dict = read_stream_dict(body);
    dict_name(&dict, "Subtype").or_else(|| {
        value_after_key(body, "Subtype").and_then(|v| match v {
            RawValue::Name(name) => Some(name),
            _ => None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_hex_decodes_pairs() {
        assert_eq!(ascii_hex_decode(b"48656C6C6F>"), b"Hello".to_vec());
        assert_eq!(ascii_hex_decode(b"4 8 6 5>"), b"He".to_vec());
    }

    #[test]
    fn ascii85_decodes_group() {
        // "Hello" in ASCII85 (the last group is zero-padded).
        assert_eq!(ascii85_decode(b"87cURDZ~>"), b"Hello".to_vec());
        assert_eq!(
            ascii85_decode(b"87cURD]i,\"Ebo80~>"),
            b"Hello World!".to_vec()
        );
    }

    #[test]
    fn run_length_decodes_literal_and_repeat() {
        // Literal run of 3 bytes, then a repeat of 'A' 3 times.
        assert_eq!(
            run_length_decode(&[2, b'x', b'y', b'z', 254, b'A']),
            b"xyzAAA".to_vec()
        );
    }

    #[test]
    fn png_predictor_none_round_trips() {
        // Two rows of four samples, each prefixed with the "None" filter byte.
        let data = vec![0u8, 1, 2, 3, 4, 0, 9, 9, 9, 9];
        let out = apply_predictor(&data, Some((12, 1, 8, 4)), 8, 4).unwrap();
        assert_eq!(out, vec![1, 2, 3, 4, 9, 9, 9, 9]);
    }

    #[test]
    fn png_predictor_up_adds_the_previous_row() {
        // Filter type 2 ("Up"): each sample adds the sample above it.
        let data = vec![2u8, 1, 2, 3, 4, 2, 1, 1, 1, 1];
        let out = apply_predictor(&data, Some((12, 1, 8, 4)), 8, 4).unwrap();
        assert_eq!(out, vec![1, 2, 3, 4, 2, 3, 4, 5]);
    }

    #[test]
    fn reads_samples_at_bit_depths() {
        // Two 2-bit samples per byte: 0b00_11_10_01 → 0, 3, 2, 1
        let data = vec![0b00_11_10_01u8];
        assert_eq!(read_sample(&data, 0, 0, 2), 0);
        assert_eq!(read_sample(&data, 0, 1, 2), 3);
        assert_eq!(read_sample(&data, 0, 2, 2), 2);
        assert_eq!(read_sample(&data, 0, 3, 2), 1);
    }
}
