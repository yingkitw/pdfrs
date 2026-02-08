use anyhow::{anyhow, Result};
use std::fs;

/// Detected image metadata
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
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

/// Load image from file, detect format, and extract dimensions
pub fn load_image(path: &str) -> Result<ImageInfo> {
    let data = fs::read(path)?;
    let format = detect_image_format(&data)?;
    let (width, height) = match format {
        ImageFormat::Jpeg => parse_jpeg_dimensions(&data)?,
        ImageFormat::Png => parse_png_dimensions(&data)?,
        ImageFormat::Bmp => parse_bmp_dimensions(&data)?,
    };
    Ok(ImageInfo { format, width, height, data })
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

    // Only JPEG is directly embeddable via DCTDecode
    if info.format != ImageFormat::Jpeg {
        return Err(anyhow!(
            "Currently only JPEG images are supported for PDF embedding. Got {:?}",
            info.format
        ));
    }

    let mut generator = crate::pdf_generator::PdfGenerator::new();

    // 1. Image XObject
    let image_id = create_jpeg_image_object(&mut generator, info.data, info.width, info.height);

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
}
