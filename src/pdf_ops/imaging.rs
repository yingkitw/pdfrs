//! Watermarks and image overlay/embedding operations.
use super::{assemble_merged_pdf, decompress_if_needed, escape_pdf_meta, extract_page_streams};
use crate::error::{PdfError, Result};
use std::fs;

pub fn create_pdf_with_images(
    output_file: &str,
    images: &[(String, f32, f32, f32, f32)], // (path, x, y, width, height)
) -> Result<()> {
    if images.is_empty() {
        return Err(PdfError::InvalidInput("No images provided".into()));
    }

    let mut generator = crate::pdf_generator::PdfGenerator::new();
    let mut image_refs: Vec<(u32, String)> = Vec::new(); // (obj_id, name)

    // Create image XObjects (supports JPEG, PNG, BMP)
    for (i, (path, _, _, _, _)) in images.iter().enumerate() {
        let info = crate::image::load_image(path)?;
        let name = format!("Im{}", i + 1);
        let image_id = crate::image::create_image_object(&mut generator, info)?;
        image_refs.push((image_id, name));
    }

    // Build content stream with all images
    let mut content = Vec::new();
    for (i, (_, x, y, w, h)) in images.iter().enumerate() {
        let name = &image_refs[i].1;
        content.extend_from_slice(b"q\n");
        content.extend_from_slice(format!("{} 0 0 {} {} {} cm\n", w, h, x, y).as_bytes());
        content.extend_from_slice(format!("/{} Do\n", name).as_bytes());
        content.extend_from_slice(b"Q\n");
    }

    let content_id =
        generator.add_stream_object(format!("<< /Length {} >>\n", content.len()), content);

    // Build XObject resource dictionary
    let xobj_entries: Vec<String> = image_refs
        .iter()
        .map(|(id, name)| format!("/{} {} 0 R", name, id))
        .collect();
    let xobj_dict = xobj_entries.join(" ");

    let page_dict = format!(
        "<< /Type /Page\n\
         /Parent 0 0 R\n\
         /MediaBox [0 0 612 792]\n\
         /Contents {} 0 R\n\
         /Resources << /XObject << {} >> >>\n\
         >>\n",
        content_id, xobj_dict
    );
    let page_id = generator.add_object(page_dict);

    let pages_dict = format!("<< /Type /Pages\n/Kids [{} 0 R]\n/Count 1\n>>\n", page_id);
    let pages_id = generator.add_object(pages_dict);

    let catalog = format!("<< /Type /Catalog\n/Pages {} 0 R\n>>\n", pages_id);
    generator.add_object(catalog);

    let pdf_data = generator.generate();
    fs::write(output_file, &pdf_data)?;
    println!(
        "[images] Created {} with {} images",
        output_file,
        images.len()
    );
    Ok(())
}

/// Add a diagonal text watermark to every page of a PDF.
///
/// The watermark is rendered as semi-transparent gray text rotated 45°.
///
/// # Arguments
///
/// * `input_file` - Path to the input PDF file
/// * `output_file` - Path where the watermarked PDF will be written
/// * `watermark_text` - Text to use as watermark
/// * `font_size` - Size of the watermark font
/// * `opacity` - Opacity of the watermark (0.0 = transparent, 1.0 = opaque)
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if watermarking fails.
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::pdf_ops;
///
/// pdf_ops::watermark_pdf(
///     "input.pdf",
///     "output.pdf",
///     "CONFIDENTIAL",
///     48.0,
///     0.3,
/// ).expect("Failed to add watermark");
/// ```
pub fn watermark_pdf(
    input_file: &str,
    output_file: &str,
    watermark_text: &str,
    font_size: f32,
    opacity: f32,
) -> Result<()> {
    let doc = crate::pdf::PdfDocument::load_from_file(input_file)?;
    let all_streams = extract_page_streams(&doc);

    if all_streams.is_empty() {
        return Err(PdfError::Parse(format!("No pages found in {}", input_file)));
    }

    let layout = crate::pdf_generator::PageLayout::portrait();
    let watermark_stream = build_watermark_stream(watermark_text, font_size, opacity, &layout);

    // Append watermark content to each page stream
    let watermarked: Vec<Vec<u8>> = all_streams
        .iter()
        .map(|stream| {
            let mut combined = stream.clone();
            combined.extend_from_slice(&watermark_stream);
            combined
        })
        .collect();

    assemble_merged_pdf(output_file, &watermarked, "Helvetica", &layout)?;
    println!(
        "[watermark] Added watermark '{}' to {} pages in {}",
        watermark_text,
        watermarked.len(),
        output_file
    );
    Ok(())
}

/// Build a content stream snippet that renders a diagonal watermark
pub(super) fn build_watermark_stream(
    text: &str,
    font_size: f32,
    opacity: f32,
    layout: &crate::pdf_generator::PageLayout,
) -> Vec<u8> {
    let escaped = escape_pdf_meta(text);
    // Center of page
    let cx = layout.width / 2.0;
    let cy = layout.height / 2.0;
    // 45° rotation matrix: cos(45)=sin(45)=1/sqrt(2)
    let cos45: f32 = std::f32::consts::FRAC_1_SQRT_2;
    let sin45: f32 = std::f32::consts::FRAC_1_SQRT_2;

    let mut stream = Vec::new();
    // Save graphics state, set transparency
    stream.extend_from_slice(b"q\n");
    stream.extend_from_slice(format!("{} {} {} rg\n", opacity, opacity, opacity).as_bytes());
    stream.extend_from_slice(b"BT\n");
    stream.extend_from_slice(format!("/F1 {} Tf\n", font_size).as_bytes());
    // Text matrix: rotation + translation to center
    stream.extend_from_slice(
        format!(
            "{} {} {} {} {} {} Tm\n",
            cos45,
            sin45,
            -sin45,
            cos45,
            cx - 100.0,
            cy - 50.0
        )
        .as_bytes(),
    );
    stream.extend_from_slice(format!("({}) Tj\n", escaped).as_bytes());
    stream.extend_from_slice(b"ET\n");
    stream.extend_from_slice(b"Q\n");
    stream
}

/// Overlay an image onto every page of a PDF.
///
/// Places an image on top of every page at the specified position and size.
/// Supports JPEG, PNG, and BMP image formats.
///
/// # Arguments
///
/// * `input_file` - Path to the input PDF file
/// * `output_file` - Path where the output PDF will be written
/// * `image_path` - Path to the image file to overlay
/// * `x` - X position of the image (in PDF points)
/// * `y` - Y position of the image (in PDF points)
/// * `width` - Width of the image (in PDF points)
/// * `height` - Height of the image (in PDF points)
/// * `opacity` - Opacity of the image (0.0 = transparent, 1.0 = opaque)
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if overlaying fails.
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::pdf_ops;
///
/// pdf_ops::overlay_image_on_pdf(
///     "input.pdf",
///     "output.pdf",
///     "logo.png",
///     100.0,  // x position
///     700.0,  // y position
///     200.0,  // width
///     100.0,  // height
///     0.8,    // opacity
/// ).expect("Failed to overlay image");
/// ```
#[allow(clippy::too_many_arguments)]
pub fn overlay_image_on_pdf(
    input_file: &str,
    output_file: &str,
    image_path: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    opacity: f32,
) -> Result<()> {
    let doc = crate::pdf::PdfDocument::load_from_file(input_file)?;
    let all_streams = extract_page_streams(&doc);

    if all_streams.is_empty() {
        return Err(PdfError::Parse(format!("No pages found in {}", input_file)));
    }

    // Load the image
    let image_info = crate::image::load_image(image_path)?;
    let mut generator = crate::pdf_generator::PdfGenerator::new();

    // Create image XObject
    let image_id = crate::image::create_image_object(&mut generator, image_info.clone())?;

    // Create overlay content stream
    let mut overlay_content = Vec::new();
    if opacity < 1.0 {
        // Set transparency
        overlay_content
            .extend_from_slice(format!("{} {} {} rg\n", opacity, opacity, opacity).as_bytes());
    }
    overlay_content.extend_from_slice(b"q\n");
    overlay_content
        .extend_from_slice(format!("{} 0 0 {} {} {} cm\n", width, height, x, y).as_bytes());
    overlay_content.extend_from_slice(b"/Im1 Do\n");
    overlay_content.extend_from_slice(b"Q\n");

    let layout = crate::pdf_generator::PageLayout::portrait();

    // For each page, append the overlay content
    let overlayed: Vec<Vec<u8>> = all_streams
        .iter()
        .map(|stream| {
            let mut combined = stream.clone();
            combined.extend_from_slice(&overlay_content);
            combined
        })
        .collect();

    // Assemble with the image XObject added to resources
    assemble_pdf_with_image_overlay(output_file, &overlayed, "Helvetica", &layout, image_id)?;
    println!(
        "[overlay] Added image overlay '{}' to {} pages in {}",
        image_path,
        overlayed.len(),
        output_file
    );
    Ok(())
}

/// Assemble PDF with image overlay XObject in resources
fn assemble_pdf_with_image_overlay(
    filename: &str,
    page_streams: &[Vec<u8>],
    font: &str,
    layout: &crate::pdf_generator::PageLayout,
    image_id: u32,
) -> Result<()> {
    let mut generator = crate::pdf_generator::PdfGenerator::new();
    let mut page_ids = Vec::new();
    let pages_obj_id = (page_streams.len() as u32) * 3 + 2;

    for page_stream in page_streams {
        let content_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", page_stream.len()),
            page_stream.clone(),
        );
        let font_id = content_id + 2;

        let page_dict = format!(
            "<< /Type /Page\n\
             /Parent {} 0 R\n\
             /MediaBox [0 0 {} {}]\n\
             /Contents {} 0 R\n\
             /Resources << /Font << /F1 {} 0 R >> /XObject << /Im1 {} 0 R >> >>\n\
             >>\n",
            pages_obj_id, layout.width, layout.height, content_id, font_id, image_id
        );
        let page_id = generator.add_object(page_dict);
        page_ids.push(page_id);

        let font_dict = format!("<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n", font);
        generator.add_object(font_dict);
    }

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let pages_dict = format!(
        "<< /Type /Pages\n/Kids [{}]\n/Count {}\n>>\n",
        kids.join(" "),
        page_ids.len()
    );
    let actual_pages_id = generator.add_object(pages_dict);
    assert_eq!(actual_pages_id, pages_obj_id);

    let catalog_dict = format!("<< /Type /Catalog\n/Pages {} 0 R\n>>\n", actual_pages_id);
    generator.add_object(catalog_dict);

    let pdf_data = generator.generate();
    let mut file = std::fs::File::create(filename)?;
    std::io::Write::write_all(&mut file, &pdf_data)?;
    Ok(())
}

/// Watermark type for different watermark styles
#[derive(Debug, Clone, Copy)]
pub enum WatermarkType {
    Text,
    Image,
}

/// Create a watermark with either text or image
pub enum WatermarkContent {
    Text(String),
    Image(String), // path to image file
}

/// Add a watermark to every page of a PDF with support for text or image watermarks
pub fn watermark_pdf_advanced(
    input_file: &str,
    output_file: &str,
    content: WatermarkContent,
    opacity: f32,
    position: WatermarkPosition,
) -> Result<()> {
    let doc = crate::pdf::PdfDocument::load_from_file(input_file)?;
    let all_streams = extract_page_streams(&doc);

    if all_streams.is_empty() {
        return Err(PdfError::Parse(format!("No pages found in {}", input_file)));
    }

    let layout = crate::pdf_generator::PageLayout::portrait();
    let watermark_stream = match content {
        WatermarkContent::Text(text) => {
            build_text_watermark_stream(&text, 48.0, opacity, &layout, position)
        }
        WatermarkContent::Image(image_path) => {
            let image_info = crate::image::load_image(&image_path)?;
            build_image_watermark_stream(&image_info, opacity, &layout, position)?
        }
    };

    // Append watermark content to each page stream
    let watermarked: Vec<Vec<u8>> = all_streams
        .iter()
        .map(|stream| {
            let mut combined = stream.clone();
            combined.extend_from_slice(&watermark_stream);
            combined
        })
        .collect();

    assemble_merged_pdf(output_file, &watermarked, "Helvetica", &layout)?;
    println!(
        "[watermark] Added watermark to {} pages in {}",
        watermarked.len(),
        output_file
    );
    Ok(())
}

/// Watermark position on the page
#[derive(Debug, Clone, Copy)]
pub enum WatermarkPosition {
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Diagonal, // Traditional diagonal watermark
}

/// Build a text watermark stream with positioning
pub(super) fn build_text_watermark_stream(
    text: &str,
    font_size: f32,
    opacity: f32,
    layout: &crate::pdf_generator::PageLayout,
    position: WatermarkPosition,
) -> Vec<u8> {
    let escaped = escape_pdf_meta(text);
    let (x, y, rotation) = match position {
        WatermarkPosition::Center => (layout.width / 2.0, layout.height / 2.0, 0.0),
        WatermarkPosition::TopLeft => (72.0, layout.height - 72.0, 0.0),
        WatermarkPosition::TopRight => (layout.width - 72.0, layout.height - 72.0, 0.0),
        WatermarkPosition::BottomLeft => (72.0, 72.0, 0.0),
        WatermarkPosition::BottomRight => (layout.width - 72.0, 72.0, 0.0),
        WatermarkPosition::Diagonal => {
            (layout.width / 2.0 - 100.0, layout.height / 2.0 - 50.0, 45.0)
        }
    };

    let mut stream = Vec::new();
    stream.extend_from_slice(b"q\n");
    stream.extend_from_slice(format!("{} {} {} rg\n", opacity, opacity, opacity).as_bytes());
    stream.extend_from_slice(b"BT\n");
    stream.extend_from_slice(format!("/F1 {} Tf\n", font_size).as_bytes());

    if rotation != 0.0 {
        let rad = rotation * std::f32::consts::PI / 180.0;
        let cos = rad.cos();
        let sin = rad.sin();
        stream.extend_from_slice(
            format!("{} {} {} {} {} {} Tm\n", cos, sin, -sin, cos, x, y).as_bytes(),
        );
    } else {
        stream.extend_from_slice(format!("{} {} Td\n", x, y).as_bytes());
    }

    stream.extend_from_slice(format!("({}) Tj\n", escaped).as_bytes());
    stream.extend_from_slice(b"ET\n");
    stream.extend_from_slice(b"Q\n");
    stream
}

/// Build an image watermark stream with positioning
pub(super) fn build_image_watermark_stream(
    image_info: &crate::image::ImageInfo,
    opacity: f32,
    layout: &crate::pdf_generator::PageLayout,
    position: WatermarkPosition,
) -> Result<Vec<u8>> {
    // Scale image to fit page if too large
    let max_width = layout.width * 0.5;
    let max_height = layout.height * 0.5;
    let (img_width, img_height) =
        crate::image::scale_to_fit(image_info.width, image_info.height, max_width, max_height);

    let (x, y) = match position {
        WatermarkPosition::Center => (
            (layout.width - img_width) / 2.0,
            (layout.height - img_height) / 2.0,
        ),
        WatermarkPosition::TopLeft => (36.0, layout.height - img_height - 36.0),
        WatermarkPosition::TopRight => (
            layout.width - img_width - 36.0,
            layout.height - img_height - 36.0,
        ),
        WatermarkPosition::BottomLeft => (36.0, 36.0),
        WatermarkPosition::BottomRight => (layout.width - img_width - 36.0, 36.0),
        WatermarkPosition::Diagonal => (
            (layout.width - img_width) / 2.0,
            (layout.height - img_height) / 2.0,
        ),
    };

    let mut stream = Vec::new();
    stream.extend_from_slice(b"q\n");
    if opacity < 1.0 {
        stream.extend_from_slice(format!("{} {} {} rg\n", opacity, opacity, opacity).as_bytes());
    }
    stream.extend_from_slice(b"q\n");
    stream
        .extend_from_slice(format!("{} 0 0 {} {} {} cm\n", img_width, img_height, x, y).as_bytes());
    stream.extend_from_slice(b"/Im1 Do\n");
    stream.extend_from_slice(b"Q\n");
    stream.extend_from_slice(b"Q\n");
    Ok(stream)
}

/// Reorder pages in a PDF according to a given order.
///
/// `page_order` is a list of 1-indexed page numbers in the desired output order.
/// Example: `[3, 1, 2]` puts page 3 first, then page 1, then page 2.
pub fn extract_images_from_pdf(input_path: &str, output_dir: &str) -> Result<Vec<String>> {
    use crate::pdf::{PdfDocument, PdfObject, PdfValue};
    use std::path::Path;

    fs::create_dir_all(output_dir)?;
    let doc = PdfDocument::load_from_file(input_path)?;

    let mut extracted = Vec::new();
    let mut image_idx = 0;

    for (obj_id, obj) in &doc.objects {
        let (dictionary, data) = match obj {
            PdfObject::Stream { dictionary, data } => (dictionary, data),
            _ => continue,
        };

        // Check if this is an image XObject
        let is_image = dictionary
            .get("Subtype")
            .and_then(|v| match v {
                PdfValue::Object(PdfObject::String(s)) => Some(s.as_str()),
                _ => None,
            })
            .map(|s| s == "/Image" || s == "Image")
            .unwrap_or(false);

        if !is_image {
            continue;
        }

        // Determine format from /Filter
        let filter = dictionary.get("Filter").and_then(|v| match v {
            PdfValue::Object(PdfObject::String(s)) => Some(s.as_str()),
            _ => None,
        });

        let (ext, raw_data) = match filter {
            Some("/DCTDecode") | Some("DCTDecode") => {
                // JPEG: data is already a valid JPEG (may need SOI marker)
                let jpeg_data = if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
                    data.clone()
                } else {
                    let mut prefixed = vec![0xFF, 0xD8];
                    prefixed.extend_from_slice(data);
                    prefixed
                };
                ("jpg", jpeg_data)
            }
            _ => {
                // Unknown or raw pixel data — decompress if needed and save as binary
                let decompressed = decompress_if_needed(data);
                ("bin", decompressed)
            }
        };

        let filename = format!("image_{:03}.{}.{}", image_idx, obj_id, ext);
        let out_path = Path::new(output_dir).join(&filename);
        fs::write(&out_path, &raw_data)?;
        extracted.push(out_path.to_string_lossy().to_string());
        image_idx += 1;
    }

    Ok(extracted)
}
