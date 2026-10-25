//! Embedding images into PDFs as image XObjects.

use super::filters::{apply_image_filters, ensure_raw_rgb};
use super::load::load_image;
use super::model::{ImageFilter, ImageFormat, ImageInfo};
use crate::error::Result;

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
