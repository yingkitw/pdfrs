//! Image loading, parsing, PDF embedding, and pixel filters
//!
//! Supports JPEG (baseline), PNG, and BMP formats. Images are parsed into
//! an [`ImageInfo`] struct containing raw pixel data and metadata, then
//! embedded into PDFs via the [`pdf_generator`](crate::pdf_generator) module.
//!
//! Pixel filters ([`ImageFilter`]) work on raw RGB (BMP, or PNG after scanline
//! reconstruction). JPEG must be converted to BMP/PNG first.

use anyhow::{Result, anyhow};
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

    /// Apply a single pixel filter, converting to raw RGB when needed.
    pub fn apply_filter(self, filter: ImageFilter) -> Result<Self> {
        apply_image_filter(self, filter)
    }

    /// Apply multiple filters in order.
    pub fn apply_filters(self, filters: &[ImageFilter]) -> Result<Self> {
        apply_image_filters(self, filters)
    }
}

/// Pixel filters for raw RGB image data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFilter {
    /// Convert to luminance grayscale
    Grayscale,
    /// Invert each color channel
    Invert,
    /// Adjust brightness by `amount` (-255..=255)
    Brightness(i16),
    /// Adjust contrast by factor (1.0 = unchanged; typical range 0.5..=2.0)
    Contrast(f32),
    /// Apply a warm sepia tone
    Sepia,
}

impl ImageFilter {
    /// Parse a filter name from CLI text (`grayscale`, `invert`, `brightness:20`, `contrast:1.5`, `sepia`).
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim().to_ascii_lowercase();
        if s == "grayscale" || s == "grey" || s == "gray" {
            return Ok(Self::Grayscale);
        }
        if s == "invert" {
            return Ok(Self::Invert);
        }
        if s == "sepia" {
            return Ok(Self::Sepia);
        }
        if let Some(rest) = s.strip_prefix("brightness:") {
            let amount: i16 = rest
                .parse()
                .map_err(|_| anyhow!("Invalid brightness amount: {rest}"))?;
            return Ok(Self::Brightness(amount.clamp(-255, 255)));
        }
        if let Some(rest) = s.strip_prefix("contrast:") {
            let factor: f32 = rest
                .parse()
                .map_err(|_| anyhow!("Invalid contrast factor: {rest}"))?;
            return Ok(Self::Contrast(factor.max(0.0)));
        }
        Err(anyhow!(
            "Unknown image filter '{s}' (expected grayscale, invert, sepia, brightness:N, contrast:F)"
        ))
    }
}

/// Convert an image to raw DeviceRGB pixels suitable for filtering and embedding.
///
/// BMP is already raw RGB. PNG scanlines are reconstructed. JPEG is not supported
/// (no built-in DCT decoder).
pub fn ensure_raw_rgb(info: ImageInfo) -> Result<ImageInfo> {
    match info.format {
        ImageFormat::Bmp => {
            if info.color_components != 3 {
                return Err(anyhow!("BMP filter support requires 3-channel RGB"));
            }
            Ok(info)
        }
        ImageFormat::Png => reconstruct_png_to_raw_rgb(info),
        ImageFormat::Jpeg => Err(anyhow!(
            "Image filters require BMP or PNG; JPEG is DCT-encoded and cannot be filtered without a JPEG decoder"
        )),
    }
}

fn reconstruct_png_to_raw_rgb(info: ImageInfo) -> Result<ImageInfo> {
    let width = info.width as usize;
    let height = info.height as usize;
    let components = match info.color_components {
        1 | 3 => info.color_components as usize,
        // Alpha already stripped during load; treat 2/4 as RGB path errors
        _ => {
            return Err(anyhow!(
                "PNG filter support requires 1 or 3 color components (got {})",
                info.color_components
            ));
        }
    };
    let row_bytes = width * components;
    let mut out = Vec::with_capacity(width * height * 3);
    let mut prev = vec![0u8; row_bytes];
    let mut i = 0usize;

    for _ in 0..height {
        if i >= info.data.len() {
            return Err(anyhow!("PNG data truncated while reconstructing scanlines"));
        }
        let filter_type = info.data[i];
        i += 1;
        if i + row_bytes > info.data.len() {
            return Err(anyhow!("PNG row truncated while reconstructing scanlines"));
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
        other => Err(anyhow!("Unsupported PNG filter type: {other}")),
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

/// Create a one-page PDF containing a filtered image.
pub fn create_filtered_image_pdf(
    image_path: &str,
    output_pdf: &str,
    filters: &[ImageFilter],
    display_width: f32,
    display_height: f32,
) -> Result<()> {
    let info = load_image(image_path)?;
    let filtered = apply_image_filters(info, filters)?;
    let (dw, dh) = scale_to_fit(
        filtered.width,
        filtered.height,
        display_width,
        display_height,
    );

    let mut generator = crate::pdf_generator::PdfGenerator::new();
    let image_id = create_image_object(&mut generator, filtered)?;
    let content = create_image_content_stream(50.0, 50.0, dw, dh, "Im1");
    let content_id =
        generator.add_stream_object(format!("<< /Length {} >>\n", content.len()), content);
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
    let pages_dict = format!("<< /Type /Pages\n/Kids [{} 0 R]\n/Count 1\n>>\n", page_id);
    let pages_id = generator.add_object(pages_dict);
    let catalog = format!("<< /Type /Catalog\n/Pages {} 0 R\n>>\n", pages_id);
    generator.add_object(catalog);
    std::fs::write(output_pdf, generator.generate())?;
    Ok(())
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
        let chunk_length =
            u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let chunk_type = &data[i + 4..i + 8];
        let chunk_data_start = i + 8;
        let chunk_data_end = chunk_data_start + chunk_length;

        if chunk_data_end > data.len() {
            return Err(anyhow!("PNG chunk data extends beyond file"));
        }

        let chunk_type_str =
            std::str::from_utf8(chunk_type).map_err(|_| anyhow!("Invalid PNG chunk type"))?;

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
        _ => {
            return Err(anyhow!(
                "Unsupported BMP bit depth: {} (only 24/32 supported)",
                bits_per_pixel
            ));
        }
    };

    // Calculate row size (BMP rows are padded to 4-byte boundaries)
    let row_size = (width as usize * bytes_per_pixel).div_ceil(4) * 4;
    let pixel_data_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;

    if pixel_data_offset + row_size * height as usize > data.len() {
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
        width,
        height,
        jpeg_data.len()
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
        width,
        height,
        bits_per_component,
        color_space,
        color_components,
        bits_per_component,
        width,
        png_data.len()
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
        width,
        height,
        bmp_data.len()
    );
    generator.add_stream_object(image_dict, bmp_data)
}

/// Create a PDF image XObject from any supported image format.
///
/// JPEG keeps `/DCTDecode`. PNG/BMP are converted to raw RGB and embedded with
/// `/FlateDecode` (no PNG predictor) so reconstructed scanlines render correctly.
pub fn create_image_object(
    generator: &mut crate::pdf_generator::PdfGenerator,
    image_info: ImageInfo,
) -> Result<u32> {
    match image_info.format {
        ImageFormat::Jpeg => Ok(create_jpeg_image_object(
            generator,
            image_info.data,
            image_info.width,
            image_info.height,
        )),
        ImageFormat::Png | ImageFormat::Bmp => {
            let raw = ensure_raw_rgb(image_info)?;
            let compressed = crate::compression::compress_deflate(&raw.data)?;
            let dict = format!(
                "<< /Type /XObject\n\
                 /Subtype /Image\n\
                 /Width {}\n\
                 /Height {}\n\
                 /BitsPerComponent 8\n\
                 /ColorSpace /DeviceRGB\n\
                 /Filter /FlateDecode\n\
                 /Length {}\n\
                 >>\n",
                raw.width,
                raw.height,
                compressed.len()
            );
            Ok(generator.add_stream_object(dict, compressed))
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
    content.extend_from_slice(format!("{} 0 0 {} {} {} cm\n", width, height, x, y).as_bytes());
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
    let content_id =
        generator.add_stream_object(format!("<< /Length {} >>\n", content.len()), content);

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
    let pages_dict = format!("<< /Type /Pages\n/Kids [{} 0 R]\n/Count 1\n>>\n", page_id);
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
    fn test_create_image_content_stream() {
        let cs = create_image_content_stream(100.0, 200.0, 300.0, 400.0, "Im1");
        let s = String::from_utf8(cs).unwrap();
        assert!(s.contains("q\n"));
        assert!(s.contains("300 0 0 400 100 200 cm"));
        assert!(s.contains("/Im1 Do"));
        assert!(s.contains("Q\n"));
    }

    fn sample_rgb_image() -> ImageInfo {
        ImageInfo {
            format: ImageFormat::Bmp,
            width: 2,
            height: 2,
            data: vec![
                255, 0, 0, // red
                0, 255, 0, // green
                0, 0, 255, // blue
                100, 100, 100, // gray
            ],
            bits_per_component: 8,
            color_components: 3,
            alt_text: None,
        }
    }

    #[test]
    fn test_image_filter_parse() {
        assert_eq!(
            ImageFilter::parse("grayscale").unwrap(),
            ImageFilter::Grayscale
        );
        assert_eq!(ImageFilter::parse("invert").unwrap(), ImageFilter::Invert);
        assert_eq!(ImageFilter::parse("sepia").unwrap(), ImageFilter::Sepia);
        assert_eq!(
            ImageFilter::parse("brightness:40").unwrap(),
            ImageFilter::Brightness(40)
        );
        assert_eq!(
            ImageFilter::parse("contrast:1.5").unwrap(),
            ImageFilter::Contrast(1.5)
        );
        assert!(ImageFilter::parse("blur").is_err());
    }

    #[test]
    fn test_grayscale_filter() {
        let filtered = sample_rgb_image()
            .apply_filter(ImageFilter::Grayscale)
            .unwrap();
        assert_eq!(filtered.data[0], filtered.data[1]);
        assert_eq!(filtered.data[1], filtered.data[2]);
        // Red luminance ≈ 0.299*255 ≈ 76
        assert!((70..=80).contains(&filtered.data[0]));
    }

    #[test]
    fn test_invert_filter() {
        let filtered = sample_rgb_image()
            .apply_filter(ImageFilter::Invert)
            .unwrap();
        assert_eq!(&filtered.data[0..3], &[0, 255, 255]); // inverted red
    }

    #[test]
    fn test_brightness_filter() {
        let filtered = sample_rgb_image()
            .apply_filter(ImageFilter::Brightness(50))
            .unwrap();
        assert_eq!(filtered.data[0], 255); // clamped
        assert_eq!(filtered.data[3], 50); // green 0 + 50
    }

    #[test]
    fn test_sepia_and_contrast_chain() {
        let filtered = sample_rgb_image()
            .apply_filters(&[ImageFilter::Sepia, ImageFilter::Contrast(1.2)])
            .unwrap();
        assert_eq!(filtered.data.len(), 12);
        assert_eq!(filtered.format, ImageFormat::Bmp);
    }

    #[test]
    fn test_jpeg_filter_rejected() {
        let jpeg = ImageInfo {
            format: ImageFormat::Jpeg,
            width: 1,
            height: 1,
            data: vec![0xFF, 0xD8, 0xFF],
            bits_per_component: 8,
            color_components: 3,
            alt_text: None,
        };
        assert!(jpeg.apply_filter(ImageFilter::Grayscale).is_err());
    }
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
