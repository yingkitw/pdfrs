use anyhow::{anyhow, Result};
use std::fs;

/// Detected image metadata
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub bits_per_component: u8,
    pub color_components: u8, // 1=grayscale, 3=RGB, 4=RGBA
    /// Alternative text for accessibility (screen readers, alt text)
    pub alt_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Bmp,
}

/// Detect format from raw bytes
pub fn detect_image_format(data: &[u8]) -> Result<ImageFormat> {
    if data.len() < 4 {
        return Err(anyhow!("Image data too short"));
    }
    if data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        Ok(ImageFormat::Jpeg)
    } else if data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47 {
        Ok(ImageFormat::Png)
    } else if data[0] == 0x42 && data[1] == 0x4D {
        Ok(ImageFormat::Bmp)
    } else {
        Err(anyhow!("Unsupported image format"))
    }
}

/// Load image from file, detect format, and extract dimensions and pixel data
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

impl ImageInfo {
    /// Set alternative text for accessibility
    pub fn with_alt_text(mut self, alt_text: String) -> Self {
        self.alt_text = Some(alt_text);
        self
    }

    /// Get the alternative text, or a default placeholder
    pub fn get_alt_text(&self) -> &str {
        self.alt_text.as_deref().unwrap_or("Image")
    }
}

/// Parse PNG IHDR chunk for width, height, bit depth, and color type
/// Returns (width, height, bits_per_component, color_components, decompressed_image_data)
fn parse_png_full(data: &[u8]) -> Result<(u32, u32, u8, u8, Vec<u8>)> {
    if data.len() < 24 {
        return Err(anyhow!("PNG data too short"));
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
        3 => return Err(anyhow!("Paletted PNG (color type 3) not yet supported")),
        4 => (2, true),
        6 => (4, true),
        _ => return Err(anyhow!("Invalid PNG color type: {}", color_type)),
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
        let chunk_length = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let chunk_type = &data[i + 4..i + 8];
        let chunk_data_start = i + 8;
        let chunk_data_end = chunk_data_start + chunk_length;

        if chunk_data_end > data.len() {
            return Err(anyhow!("PNG chunk data extends beyond file"));
        }

        let chunk_type_str = std::str::from_utf8(chunk_type)
            .map_err(|_| anyhow!("Invalid PNG chunk type"))?;

        if chunk_type_str == "IDAT" {
            idat_data.extend_from_slice(&data[chunk_data_start..chunk_data_end]);
        } else if chunk_type_str == "IEND" {
            break;
        }

        // Skip to next chunk (length + type + data + CRC)
        i = chunk_data_end + 4; // +4 for CRC
    }

    if idat_data.is_empty() {
        return Err(anyhow!("No IDAT chunks found in PNG"));
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
            return Err(anyhow!("PNG data truncated"));
        }
        let filter = data[i];
        i += 1;

        if i + row_size > data.len() {
            return Err(anyhow!("PNG row data truncated"));
        }

        // Copy filter byte
        result.push(filter);

        // Copy pixel data, skipping alpha
        let mut pixel_start = i;
        for _ in 0..width as usize {
            if pixel_start + components > data.len() {
                return Err(anyhow!("PNG pixel data truncated"));
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
fn parse_jpeg_dimensions(data: &[u8]) -> Result<(u32, u32)> {
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
                return Err(anyhow!("JPEG SOF marker truncated"));
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
    Err(anyhow!("Could not find JPEG SOF marker"))
}

/// Parse PNG IHDR chunk for width and height
fn parse_png_dimensions(data: &[u8]) -> Result<(u32, u32)> {
    // PNG header: 8 bytes, then IHDR chunk: 4-byte length, 4-byte type, then data
    if data.len() < 24 {
        return Err(anyhow!("PNG data too short"));
    }
    // IHDR starts at offset 8 (after signature)
    // bytes 8..12 = chunk length, 12..16 = "IHDR", 16..20 = width, 20..24 = height
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    Ok((width, height))
}

/// Parse BMP header for width and height
fn parse_bmp_dimensions(data: &[u8]) -> Result<(u32, u32)> {
    if data.len() < 26 {
        return Err(anyhow!("BMP data too short"));
    }
    // BMP info header starts at offset 14; width at +4, height at +8 (little-endian i32)
    let width = u32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let height_raw = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let height = height_raw.unsigned_abs();
    Ok((width, height))
}

/// Parse BMP full data: extract dimensions, bit depth, and pixel data
/// Returns (width, height, bits_per_component, color_components, pixel_data)
fn parse_bmp_full(data: &[u8]) -> Result<(u32, u32, u8, u8, Vec<u8>)> {
    if data.len() < 54 {
        return Err(anyhow!("BMP data too short for header"));
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
        _ => return Err(anyhow!("Unsupported BMP bit depth: {} (only 24/32 supported)", bits_per_pixel)),
    };

    // Calculate row size (BMP rows are padded to 4-byte boundaries)
    let row_size = ((width as usize * bytes_per_pixel + 3) / 4) * 4;
    let pixel_data_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;

    if pixel_data_offset as usize + row_size * height as usize > data.len() {
        return Err(anyhow!("BMP pixel data truncated"));
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

/// Scale dimensions to fit within max_width x max_height while preserving aspect ratio
pub fn scale_to_fit(width: u32, height: u32, max_width: f32, max_height: f32) -> (f32, f32) {
    let w = width as f32;
    let h = height as f32;
    let scale_w = max_width / w;
    let scale_h = max_height / h;
    let scale = scale_w.min(scale_h).min(1.0); // don't upscale
    (w * scale, h * scale)
}

/// Create a PDF image XObject stream for JPEG data (DCTDecode)
pub fn create_jpeg_image_object(
    generator: &mut crate::pdf_generator::PdfGenerator,
    jpeg_data: Vec<u8>,
    width: u32,
    height: u32,
) -> u32 {
    let image_dict = format!(
        "<< /Type /XObject\n\
         /Subtype /Image\n\
         /Width {}\n\
         /Height {}\n\
         /BitsPerComponent 8\n\
         /ColorSpace /DeviceRGB\n\
         /Filter /DCTDecode\n\
         /Length {}\n\
         >>\n",
        width, height, jpeg_data.len()
    );
    generator.add_stream_object(image_dict, jpeg_data)
}

/// Create a PDF image XObject stream for PNG data (FlateDecode)
pub fn create_png_image_object(
    generator: &mut crate::pdf_generator::PdfGenerator,
    png_data: Vec<u8>,
    width: u32,
    height: u32,
    bits_per_component: u8,
    color_components: u8,
) -> u32 {
    // Determine color space
    let color_space = match color_components {
        1 => "/DeviceGray",
        3 => "/DeviceRGB",
        _ => "/DeviceRGB", // Fallback
    };

    let image_dict = format!(
        "<< /Type /XObject\n\
         /Subtype /Image\n\
         /Width {}\n\
         /Height {}\n\
         /BitsPerComponent {}\n\
         /ColorSpace {}\n\
         /Filter /FlateDecode\n\
         /DecodeParms << /Predictor 15 /Colors {} /BitsPerComponent {} /Columns {} >>\n\
         /Length {}\n\
         >>\n",
        width, height, bits_per_component, color_space,
        color_components, bits_per_component, width, png_data.len()
    );
    generator.add_stream_object(image_dict, png_data)
}

/// Create a PDF image XObject stream for BMP data (raw, no filter)
pub fn create_bmp_image_object(
    generator: &mut crate::pdf_generator::PdfGenerator,
    bmp_data: Vec<u8>,
    width: u32,
    height: u32,
) -> u32 {
    let image_dict = format!(
        "<< /Type /XObject\n\
         /Subtype /Image\n\
         /Width {}\n\
         /Height {}\n\
         /BitsPerComponent 8\n\
         /ColorSpace /DeviceRGB\n\
         /Length {}\n\
         >>\n",
        width, height, bmp_data.len()
    );
    generator.add_stream_object(image_dict, bmp_data)
}

/// Create a PDF image XObject from any supported image format
pub fn create_image_object(
    generator: &mut crate::pdf_generator::PdfGenerator,
    image_info: ImageInfo,
) -> Result<u32> {
    match image_info.format {
        ImageFormat::Jpeg => {
            Ok(create_jpeg_image_object(
                generator,
                image_info.data,
                image_info.width,
                image_info.height,
            ))
        }
        ImageFormat::Png => {
            Ok(create_png_image_object(
                generator,
                image_info.data,
                image_info.width,
                image_info.height,
                image_info.bits_per_component,
                image_info.color_components,
            ))
        }
        ImageFormat::Bmp => {
            Ok(create_bmp_image_object(
                generator,
                image_info.data,
                image_info.width,
                image_info.height,
            ))
        }
    }
}

/// Create content stream that draws an image XObject
pub fn create_image_content_stream(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    image_name: &str,
) -> Vec<u8> {
    let mut content = Vec::new();
    content.extend_from_slice(b"q\n");
    content.extend_from_slice(
        format!("{} 0 0 {} {} {} cm\n", width, height, x, y).as_bytes(),
    );
    content.extend_from_slice(format!("/{} Do\n", image_name).as_bytes());
    content.extend_from_slice(b"Q\n");
    content
}

/// High-level: create a single-page PDF containing just the image
pub fn add_image_to_pdf(
    output_pdf: &str,
    image_path: &str,
    x: f32,
    y: f32,
    display_width: f32,
    display_height: f32,
) -> Result<()> {
    let info = load_image(image_path)?;

    let mut generator = crate::pdf_generator::PdfGenerator::new();

    // 1. Image XObject (supports JPEG, PNG, BMP)
    let image_id = create_image_object(&mut generator, info.clone())?;

    // 2. Content stream that draws the image
    let content = create_image_content_stream(x, y, display_width, display_height, "Im1");
    let content_id = generator.add_stream_object(
        format!("<< /Length {} >>\n", content.len()),
        content,
    );

    // 3. Page object
    let page_dict = format!(
        "<< /Type /Page\n\
         /Parent 5 0 R\n\
         /MediaBox [0 0 612 792]\n\
         /Contents {} 0 R\n\
         /Resources << /XObject << /Im1 {} 0 R >> >>\n\
         >>\n",
        content_id, image_id
    );
    let page_id = generator.add_object(page_dict);

    // 4. Pages
    let pages_dict = format!(
        "<< /Type /Pages\n/Kids [{} 0 R]\n/Count 1\n>>\n",
        page_id
    );
    let pages_id = generator.add_object(pages_dict);

    // 5. Catalog
    let catalog = format!("<< /Type /Catalog\n/Pages {} 0 R\n>>\n", pages_id);
    generator.add_object(catalog);

    let pdf_data = generator.generate();
    std::fs::write(output_pdf, &pdf_data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_jpeg() {
        let data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00];
        assert_eq!(detect_image_format(&data).unwrap(), ImageFormat::Jpeg);
    }

    #[test]
    fn test_detect_png() {
        let data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D];
        assert_eq!(detect_image_format(&data).unwrap(), ImageFormat::Png);
    }

    #[test]
    fn test_detect_bmp() {
        let data = vec![0x42, 0x4D, 0x00, 0x00];
        assert_eq!(detect_image_format(&data).unwrap(), ImageFormat::Bmp);
    }

    #[test]
    fn test_detect_unknown() {
        let data = vec![0x00, 0x00, 0x00, 0x00];
        assert!(detect_image_format(&data).is_err());
    }

    #[test]
    fn test_scale_to_fit() {
        // Image 800x600, max 400x400 -> scale by 0.5 -> 400x300
        let (w, h) = scale_to_fit(800, 600, 400.0, 400.0);
        assert!((w - 400.0).abs() < 0.01);
        assert!((h - 300.0).abs() < 0.01);
    }

    #[test]
    fn test_scale_no_upscale() {
        // Image 100x50, max 400x400 -> no upscale -> 100x50
        let (w, h) = scale_to_fit(100, 50, 400.0, 400.0);
        assert!((w - 100.0).abs() < 0.01);
        assert!((h - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_jpeg_dimensions() {
        // Minimal JPEG with SOF0 marker: FF D8 FF C0 00 11 08 <H:2> <W:2> ...
        let mut data = vec![0xFF, 0xD8]; // SOI
        // APP0 marker (skip)
        data.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00]);
        // SOF0 marker
        data.extend_from_slice(&[0xFF, 0xC0]);
        data.extend_from_slice(&[0x00, 0x11]); // length
        data.push(0x08); // precision
        data.extend_from_slice(&[0x01, 0x00]); // height = 256
        data.extend_from_slice(&[0x02, 0x00]); // width = 512
        data.extend_from_slice(&[0x03]); // components
        // pad
        data.extend_from_slice(&[0; 20]);

        let (w, h) = parse_jpeg_dimensions(&data).unwrap();
        assert_eq!(w, 512);
        assert_eq!(h, 256);
    }

    #[test]
    fn test_parse_png_dimensions() {
        // Minimal PNG header + IHDR
        let mut data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // signature
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x0D]); // IHDR length
        data.extend_from_slice(b"IHDR");
        data.extend_from_slice(&640u32.to_be_bytes()); // width
        data.extend_from_slice(&480u32.to_be_bytes()); // height

        let (w, h) = parse_png_dimensions(&data).unwrap();
        assert_eq!(w, 640);
        assert_eq!(h, 480);
    }

    #[test]
    fn test_create_image_content_stream() {
        let cs = create_image_content_stream(100.0, 200.0, 300.0, 400.0, "Im1");
        let s = String::from_utf8(cs).unwrap();
        assert!(s.contains("q\n"));
        assert!(s.contains("300 0 0 400 100 200 cm"));
        assert!(s.contains("/Im1 Do"));
        assert!(s.contains("Q\n"));
    }

    #[test]
    fn test_png_color_components() {
        // Test color type 0 (grayscale)
        assert_eq!(get_png_color_components(0), Some((1, false)));
        // Test color type 2 (RGB)
        assert_eq!(get_png_color_components(2), Some((3, false)));
        // Test color type 4 (grayscale + alpha)
        assert_eq!(get_png_color_components(4), Some((2, true)));
        // Test color type 6 (RGB + alpha)
        assert_eq!(get_png_color_components(6), Some((4, true)));
        // Test invalid color types
        assert_eq!(get_png_color_components(1), None);
        assert_eq!(get_png_color_components(5), None);
    }

    #[test]
    fn test_bmp_bit_depth_validation() {
        // Test valid bit depths
        assert!(validate_bmp_bit_depth(24).is_ok());
        assert!(validate_bmp_bit_depth(32).is_ok());
        // Test invalid bit depths
        assert!(validate_bmp_bit_depth(8).is_err());
        assert!(validate_bmp_bit_depth(16).is_err());
        assert!(validate_bmp_bit_depth(1).is_err());
    }

    #[test]
    fn test_bmp_row_padding() {
        // BMP rows are padded to 4-byte boundaries
        // Width 1 pixel, 3 bytes per pixel (24-bit) = 3 bytes, padded to 4 bytes
        let row_size = calculate_bmp_row_size(1, 3);
        assert_eq!(row_size, 4);

        // Width 2 pixels, 3 bytes per pixel = 6 bytes, padded to 8 bytes
        let row_size = calculate_bmp_row_size(2, 3);
        assert_eq!(row_size, 8);

        // Width 3 pixels, 3 bytes per pixel = 9 bytes, padded to 12 bytes
        let row_size = calculate_bmp_row_size(3, 3);
        assert_eq!(row_size, 12);

        // Width 4 pixels, 3 bytes per pixel = 12 bytes, no padding needed
        let row_size = calculate_bmp_row_size(4, 3);
        assert_eq!(row_size, 12);
    }
}

/// Helper function to get PNG color components from color type
fn get_png_color_components(color_type: u8) -> Option<(u8, bool)> {
    match color_type {
        0 => Some((1, false)),    // grayscale
        2 => Some((3, false)),    // RGB
        4 => Some((2, true)),     // grayscale + alpha
        6 => Some((4, true)),     // RGB + alpha
        _ => None,
    }
}

/// Helper function to validate BMP bit depth
fn validate_bmp_bit_depth(bits_per_pixel: u16) -> Result<()> {
    match bits_per_pixel {
        24 | 32 => Ok(()),
        _ => Err(anyhow!("Unsupported BMP bit depth: {}", bits_per_pixel)),
    }
}

/// Helper function to calculate BMP row size with padding
fn calculate_bmp_row_size(width: u32, bytes_per_pixel: u8) -> usize {
    let row_size = width as usize * bytes_per_pixel as usize;
    ((row_size + 3) / 4) * 4
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn scale_preserves_aspect_ratio(width in 1u32..4000u32, height in 1u32..4000u32,
                                    max_w in 100f32..2000f32, max_h in 100f32..2000f32) {
            let (scaled_w, scaled_h) = scale_to_fit(width, height, max_w, max_h);

            // Check that scaled dimensions don't exceed max
            assert!(scaled_w <= max_w + 0.01, "Scaled width exceeds max");
            assert!(scaled_h <= max_h + 0.01, "Scaled height exceeds max");

            // Check that aspect ratio is preserved (within tolerance)
            let original_aspect = width as f32 / height as f32;
            let scaled_aspect = scaled_w / scaled_h;
            assert!((original_aspect - scaled_aspect).abs() < 0.01f32, "Aspect ratio not preserved");

            // Check that we don't upscale
            assert!(scaled_w <= width as f32 + 0.01, "Width was upscaled");
            assert!(scaled_h <= height as f32 + 0.01, "Height was upscaled");
        }
    }

    proptest! {
        #[test]
        fn scale_never_exceeds_bounds(width in 1u32..4000u32, height in 1u32..4000u32,
                                   max_w in 100f32..2000f32, max_h in 100f32..2000f32) {
            let (scaled_w, scaled_h) = scale_to_fit(width, height, max_w, max_h);
            // Allow small tolerance for floating point precision
            assert!(scaled_w <= max_w + 0.01, "Scaled width {} exceeds max_w {}", scaled_w, max_w);
            assert!(scaled_h <= max_h + 0.01, "Scaled height {} exceeds max_h {}", scaled_h, max_h);
        }
    }
}
