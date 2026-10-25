//! Pixel filters and PNG scanline reconstruction.

use super::model::{ImageFilter, ImageFormat, ImageInfo};
use crate::error::{PdfError, Result};

/// Convert an image to raw DeviceRGB pixels suitable for filtering and embedding.
///
/// BMP is already raw RGB. PNG scanlines are reconstructed. JPEG is not supported
/// (no built-in DCT decoder).
pub fn ensure_raw_rgb(info: ImageInfo) -> Result<ImageInfo> {
    match info.format {
        ImageFormat::Bmp => {
            if info.color_components != 3 {
                return Err(PdfError::Image("BMP filter support requires 3-channel RGB".into()));
            }
            Ok(info)
        }
        ImageFormat::Png => reconstruct_png_to_raw_rgb(info),
        ImageFormat::Jpeg => Err(PdfError::Image(
            "Image filters require BMP or PNG; JPEG is DCT-encoded and cannot be filtered without a JPEG decoder"
        .into())),
    }
}

fn reconstruct_png_to_raw_rgb(info: ImageInfo) -> Result<ImageInfo> {
    let width = info.width as usize;
    let height = info.height as usize;
    let components = match info.color_components {
        1 | 3 => info.color_components as usize,
        // Alpha already stripped during load; treat 2/4 as RGB path errors
        _ => {
            return Err(PdfError::Image(format!(
                "PNG filter support requires 1 or 3 color components (got {})",
                info.color_components
            )));
        }
    };
    let row_bytes = width * components;
    let mut out = Vec::with_capacity(width * height * 3);
    let mut prev = vec![0u8; row_bytes];
    let mut i = 0usize;

    for _ in 0..height {
        if i >= info.data.len() {
            return Err(PdfError::Image(
                "PNG data truncated while reconstructing scanlines".into(),
            ));
        }
        let filter_type = info.data[i];
        i += 1;
        if i + row_bytes > info.data.len() {
            return Err(PdfError::Image(
                "PNG row truncated while reconstructing scanlines".into(),
            ));
        }
        let mut cur = info.data[i..i + row_bytes].to_vec();
        i += row_bytes;
        apply_png_filter(filter_type, &mut cur, &prev, components)?;
        for px in cur.chunks(components) {
            match components {
                1 => {
                    out.push(px[0]);
                    out.push(px[0]);
                    out.push(px[0]);
                }
                3 => {
                    out.extend_from_slice(px);
                }
                _ => unreachable!(),
            }
        }
        prev = cur;
    }

    Ok(ImageInfo {
        format: ImageFormat::Bmp,
        width: info.width,
        height: info.height,
        data: out,
        bits_per_component: 8,
        color_components: 3,
        alt_text: info.alt_text,
    })
}

fn apply_png_filter(filter_type: u8, cur: &mut [u8], prev: &[u8], bpp: usize) -> Result<()> {
    match filter_type {
        0 => Ok(()), // None
        1 => {
            // Sub
            for i in bpp..cur.len() {
                cur[i] = cur[i].wrapping_add(cur[i - bpp]);
            }
            Ok(())
        }
        2 => {
            // Up
            for i in 0..cur.len() {
                cur[i] = cur[i].wrapping_add(prev[i]);
            }
            Ok(())
        }
        3 => {
            // Average
            for i in 0..cur.len() {
                let left = if i >= bpp { cur[i - bpp] } else { 0 };
                let up = prev[i];
                cur[i] = cur[i].wrapping_add(((u16::from(left) + u16::from(up)) / 2) as u8);
            }
            Ok(())
        }
        4 => {
            // Paeth
            for i in 0..cur.len() {
                let a = if i >= bpp { cur[i - bpp] } else { 0 };
                let b = prev[i];
                let c = if i >= bpp { prev[i - bpp] } else { 0 };
                cur[i] = cur[i].wrapping_add(paeth_predictor(a, b, c));
            }
            Ok(())
        }
        other => Err(PdfError::Unsupported(format!(
            "Unsupported PNG filter type: {other}"
        ))),
    }
}

fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let a = i16::from(a);
    let b = i16::from(b);
    let c = i16::from(c);
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

/// Apply one filter to an image (converts to raw RGB first when needed).
pub fn apply_image_filter(info: ImageInfo, filter: ImageFilter) -> Result<ImageInfo> {
    let mut info = ensure_raw_rgb(info)?;
    apply_filter_to_rgb(&mut info.data, filter);
    if matches!(filter, ImageFilter::Grayscale) {
        // Keep 3-channel gray RGB for DeviceRGB embedding consistency
        info.color_components = 3;
    }
    Ok(info)
}

/// Apply filters sequentially.
pub fn apply_image_filters(info: ImageInfo, filters: &[ImageFilter]) -> Result<ImageInfo> {
    let mut info = ensure_raw_rgb(info)?;
    for filter in filters {
        apply_filter_to_rgb(&mut info.data, *filter);
    }
    Ok(info)
}

fn apply_filter_to_rgb(data: &mut [u8], filter: ImageFilter) {
    match filter {
        ImageFilter::Grayscale => {
            for px in data.chunks_exact_mut(3) {
                let y = ((77u32 * u32::from(px[0])
                    + 150u32 * u32::from(px[1])
                    + 29u32 * u32::from(px[2]))
                    >> 8) as u8;
                px[0] = y;
                px[1] = y;
                px[2] = y;
            }
        }
        ImageFilter::Invert => {
            for b in data.iter_mut() {
                *b = 255 - *b;
            }
        }
        ImageFilter::Brightness(amount) => {
            for b in data.iter_mut() {
                *b = (i16::from(*b) + amount).clamp(0, 255) as u8;
            }
        }
        ImageFilter::Contrast(factor) => {
            for b in data.iter_mut() {
                let v = (f32::from(*b) - 128.0) * factor + 128.0;
                *b = v.clamp(0.0, 255.0) as u8;
            }
        }
        ImageFilter::Sepia => {
            for px in data.chunks_exact_mut(3) {
                let r = f32::from(px[0]);
                let g = f32::from(px[1]);
                let b = f32::from(px[2]);
                px[0] = (0.393 * r + 0.769 * g + 0.189 * b).clamp(0.0, 255.0) as u8;
                px[1] = (0.349 * r + 0.686 * g + 0.168 * b).clamp(0.0, 255.0) as u8;
                px[2] = (0.272 * r + 0.534 * g + 0.131 * b).clamp(0.0, 255.0) as u8;
            }
        }
    }
}
