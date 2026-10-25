//! Loading images from disk and parsing their container formats.

use std::fs;

use super::model::{ImageFormat, ImageInfo, detect_image_format};
use crate::error::{PdfError, Result};

pub fn load_image(path: &str) -> Result<ImageInfo> {
    load_image_with_alt_text(path, None)
}

/// Load image from file with alternative text for accessibility
pub fn load_image_with_alt_text(path: &str, alt_text: Option<String>) -> Result<ImageInfo> {
    let data = fs::read(path)?;
    let format = detect_image_format(&data)?;
    let (width, height, bits_per_comp, color_comp, pixel_data) = match format {
        ImageFormat::Jpeg => {
            let (w, h) = parse_jpeg_dimensions(&data)?;
            (w, h, 8, 3, data)
        }
        ImageFormat::Png => parse_png_full(&data)?,
        ImageFormat::Bmp => parse_bmp_full(&data)?,
    };
    Ok(ImageInfo {
        format,
        width,
        height,
        data: pixel_data,
        bits_per_component: bits_per_comp,
        color_components: color_comp,
        alt_text,
    })
}

fn parse_png_full(data: &[u8]) -> Result<(u32, u32, u8, u8, Vec<u8>)> {
    if data.len() < 24 {
        return Err(PdfError::Image("PNG data too short".into()));
    }

    // PNG header: 8 bytes
    // IHDR chunk: 4-byte length, 4-byte type, then data
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    let bit_depth = data[24];
    let color_type = data[25];

    // Determine color components from color type
    // 0 = grayscale (1 component)
    // 2 = RGB (3 components)
    // 3 = palette (1 component, but needs special handling)
    // 4 = grayscale + alpha (2 components)
    // 6 = RGB + alpha (4 components)
    let (color_components, has_alpha) = match color_type {
        0 => (1, false),
        2 => (3, false),
        3 => {
            return Err(PdfError::Image(
                "Paletted PNG (color type 3) not yet supported".into(),
            ));
        }
        4 => (2, true),
        6 => (4, true),
        _ => {
            return Err(PdfError::Image(format!(
                "Invalid PNG color type: {}",
                color_type
            )));
        }
    };

    // Collect all IDAT chunks and decompress
    let idat_data = extract_png_idat_chunks(data)?;
    let decompressed = decompress_png_data(&idat_data)?;

    // Remove alpha channel if present (PDF doesn't support alpha in basic images)
    let final_data = if has_alpha {
        remove_alpha_channel(&decompressed, color_components, width, height)?
    } else {
        decompressed
    };

    Ok((width, height, bit_depth, color_components, final_data))
}

/// Extract all IDAT chunk data from PNG
fn extract_png_idat_chunks(data: &[u8]) -> Result<Vec<u8>> {
    let mut idat_data = Vec::new();
    let mut i = 8; // Skip PNG signature

    while i + 8 <= data.len() {
        let chunk_length =
            u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let chunk_type = &data[i + 4..i + 8];
        let chunk_data_start = i + 8;
        let chunk_data_end = chunk_data_start + chunk_length;

        if chunk_data_end > data.len() {
            return Err(PdfError::Image("PNG chunk data extends beyond file".into()));
        }

        let chunk_type_str = std::str::from_utf8(chunk_type)
            .map_err(|_| PdfError::Image("Invalid PNG chunk type".into()))?;

        if chunk_type_str == "IDAT" {
            idat_data.extend_from_slice(&data[chunk_data_start..chunk_data_end]);
        } else if chunk_type_str == "IEND" {
            break;
        }

        // Skip to next chunk (length + type + data + CRC)
        i = chunk_data_end + 4; // +4 for CRC
    }

    if idat_data.is_empty() {
        return Err(PdfError::Image("No IDAT chunks found in PNG".into()));
    }

    Ok(idat_data)
}

/// Decompress PNG IDAT data using deflate
fn decompress_png_data(compressed: &[u8]) -> Result<Vec<u8>> {
    // PNG uses zlib compression (deflate with wrapper)
    // For now, use the compression module's decompress function
    // In a production implementation, you'd use flate2 with proper zlib handling
    crate::compression::decompress_deflate(compressed)
}

/// Remove alpha channel from image data
fn remove_alpha_channel(data: &[u8], components: u8, width: u32, height: u32) -> Result<Vec<u8>> {
    let components = components as usize;
    let bytes_per_pixel = components;
    let _stride = width as usize * bytes_per_pixel + 1; // +1 for filter byte per row
    let row_size = width as usize * components;

    let mut result = Vec::new();
    let mut i = 0;

    for _ in 0..height {
        if i + 1 > data.len() {
            return Err(PdfError::Image("PNG data truncated".into()));
        }
        let filter = data[i];
        i += 1;

        if i + row_size > data.len() {
            return Err(PdfError::Image("PNG row data truncated".into()));
        }

        // Copy filter byte
        result.push(filter);

        // Copy pixel data, skipping alpha
        let mut pixel_start = i;
        for _ in 0..width as usize {
            if pixel_start + components > data.len() {
                return Err(PdfError::Image("PNG pixel data truncated".into()));
            }
            // Copy RGB components, skip alpha
            for c in 0..3 {
                if c < components - 1 {
                    // Keep only RGB, drop alpha
                    result.push(data[pixel_start + c]);
                }
            }
            pixel_start += components;
        }

        i += row_size;
    }

    Ok(result)
}

/// Parse JPEG SOF marker to get width and height
pub(super) fn parse_jpeg_dimensions(data: &[u8]) -> Result<(u32, u32)> {
    let mut i = 2; // skip FF D8
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        i += 2;

        // SOF0..SOF15 (except SOF4 = DHT, SOF8 = JPG)
        // Common SOF markers: C0, C1, C2
        if marker == 0xC0 || marker == 0xC1 || marker == 0xC2 {
            if i + 7 > data.len() {
                return Err(PdfError::Image("JPEG SOF marker truncated".into()));
            }
            let height = ((data[i + 3] as u32) << 8) | (data[i + 4] as u32);
            let width = ((data[i + 5] as u32) << 8) | (data[i + 6] as u32);
            return Ok((width, height));
        }

        // Skip non-SOF markers by reading their length
        if i + 1 >= data.len() {
            break;
        }
        let seg_len = ((data[i] as usize) << 8) | (data[i + 1] as usize);
        i += seg_len;
    }
    Err(PdfError::Image("Could not find JPEG SOF marker".into()))
}

/// Parse BMP full data: extract dimensions, bit depth, and pixel data
/// Returns (width, height, bits_per_component, color_components, pixel_data)
fn parse_bmp_full(data: &[u8]) -> Result<(u32, u32, u8, u8, Vec<u8>)> {
    if data.len() < 54 {
        return Err(PdfError::Image("BMP data too short for header".into()));
    }

    // BMP file header (14 bytes) + info header (40 bytes for BITMAPINFOHEADER)
    // Width at offset 18, height at offset 22, bit depth at offset 28
    let width = u32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let height_raw = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let height = height_raw.unsigned_abs();
    let bits_per_pixel = u16::from_le_bytes([data[28], data[29]]);

    // Only support 24-bit and 32-bit BMPs
    let (bytes_per_pixel, _has_alpha) = match bits_per_pixel {
        24 => (3, false),
        32 => (4, true),
        _ => {
            return Err(PdfError::Unsupported(format!(
                "Unsupported BMP bit depth: {} (only 24/32 supported)",
                bits_per_pixel
            )));
        }
    };

    // Calculate row size (BMP rows are padded to 4-byte boundaries)
    let row_size = (width as usize * bytes_per_pixel).div_ceil(4) * 4;
    let pixel_data_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;

    if pixel_data_offset + row_size * height as usize > data.len() {
        return Err(PdfError::Image("BMP pixel data truncated".into()));
    }

    // Extract pixel data, flipping vertically (BMP stores bottom-to-top)
    let mut pixel_data = Vec::with_capacity((width * height * 3) as usize);
    for y in (0..height as usize).rev() {
        let row_start = pixel_data_offset + y * row_size;
        for x in 0..width as usize {
            let pixel_start = row_start + x * bytes_per_pixel;
            // BMP is stored in BGR order, convert to RGB
            let b = data[pixel_start];
            let g = data[pixel_start + 1];
            let r = data[pixel_start + 2];
            pixel_data.push(r);
            pixel_data.push(g);
            pixel_data.push(b);
        }
    }

    Ok((width, height, 8, 3, pixel_data))
}
