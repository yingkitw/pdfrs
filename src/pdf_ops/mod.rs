//! PDF high-level operations module
//!
//! This module provides high-level operations for manipulating PDF documents,
//! including merging, splitting, rotating, watermarking, and annotations.

mod annotations;
mod forms;
mod metadata;
mod portfolio;
mod security;
mod structure;
mod tables;

pub use annotations::*;
pub use forms::*;
pub use metadata::*;
pub use portfolio::*;
pub use security::*;
pub use structure::*;
pub use tables::*;

use anyhow::{Result, anyhow};
use std::fs;

/// Merge multiple PDF files into a single output PDF.
///
/// This function extracts page content from each input PDF and combines them
/// into a single output PDF, preserving the order of input files.
///
/// # Arguments
///
/// * `input_files` - Slice of file paths to merge
/// * `output_file` - Path where the merged PDF will be written
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if merging fails.
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::pdf_ops;
///
/// pdf_ops::merge_pdfs(
///     &["file1.pdf", "file2.pdf", "file3.pdf"],
///     "merged.pdf",
/// ).expect("Failed to merge PDFs");
/// ```
///
/// # Errors
///
/// This function will return an error if:
/// - No input files are provided
/// - Any input file cannot be read or parsed
/// - No page content is found in any input file
pub fn merge_pdfs(input_files: &[&str], output_file: &str) -> Result<()> {
    if input_files.is_empty() {
        return Err(anyhow!("No input files provided for merge"));
    }

    let mut all_page_streams: Vec<Vec<u8>> = Vec::new();

    for path in input_files {
        let doc = crate::pdf::PdfDocument::load_from_file(path)?;
        let streams = extract_page_streams(&doc);
        if streams.is_empty() {
            eprintln!("[merge] Warning: no page streams found in {}", path);
        }
        all_page_streams.extend(streams);
    }

    if all_page_streams.is_empty() {
        return Err(anyhow!("No page content found in any input file"));
    }

    let layout = crate::pdf_generator::PageLayout::portrait();
    assemble_merged_pdf(output_file, &all_page_streams, "Helvetica", &layout)?;
    println!(
        "[merge] Combined {} pages from {} files into {}",
        all_page_streams.len(),
        input_files.len(),
        output_file
    );
    Ok(())
}

/// Merge multiple already-loaded PdfDocument instances into a single output PDF.
///
/// This is a helper function for parallel PDF operations where documents
/// have already been loaded concurrently. It extracts page content from each
/// PdfDocument and combines them into a single output PDF.
///
/// # Arguments
///
/// * `documents` - Slice of already-loaded PdfDocument instances
/// * `output_file` - Path where the merged PDF will be written
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if merging fails.
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::pdf_ops;
/// use pdfrs::pdf::PdfDocument;
///
/// let doc1 = PdfDocument::load_from_file("file1.pdf")?;
/// let doc2 = PdfDocument::load_from_file("file2.pdf")?;
/// let docs = vec![doc1, doc2];
///
/// pdf_ops::merge_pdfs_sequential(&docs, "merged.pdf")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn merge_pdfs_sequential(
    documents: &[crate::pdf::PdfDocument],
    output_file: &str,
) -> Result<()> {
    if documents.is_empty() {
        return Err(anyhow!("No documents provided for merge"));
    }

    let mut all_page_streams: Vec<Vec<u8>> = Vec::new();

    for doc in documents {
        let streams = extract_page_streams(doc);
        if streams.is_empty() {
            eprintln!("[merge] Warning: no page streams found in document");
        }
        all_page_streams.extend(streams);
    }

    if all_page_streams.is_empty() {
        return Err(anyhow!("No page content found in any document"));
    }

    let layout = crate::pdf_generator::PageLayout::portrait();
    assemble_merged_pdf(output_file, &all_page_streams, "Helvetica", &layout)?;
    println!(
        "[merge] Combined {} pages from {} documents into {}",
        all_page_streams.len(),
        documents.len(),
        output_file
    );
    Ok(())
}

/// Split a PDF by extracting a range of pages into a new PDF.
///
/// Extracts pages from `start` to `end` (inclusive, 1-indexed) and creates
/// a new PDF containing only those pages.
///
/// # Arguments
///
/// * `input_file` - Path to the input PDF file
/// * `output_file` - Path where the split PDF will be written
/// * `start` - Starting page number (1-indexed)
/// * `end` - Ending page number (1-indexed, inclusive)
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if splitting fails.
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::pdf_ops;
///
/// // Extract pages 3-7 into a new PDF
/// pdf_ops::split_pdf("input.pdf", "output.pdf", 3, 7)
///     .expect("Failed to split PDF");
/// ```
pub fn split_pdf(input_file: &str, output_file: &str, start: usize, end: usize) -> Result<()> {
    if start == 0 || end == 0 || start > end {
        return Err(anyhow!(
            "Invalid page range: start={} end={} (1-indexed, inclusive)",
            start,
            end
        ));
    }

    let doc = crate::pdf::PdfDocument::load_from_file(input_file)?;
    let all_streams = extract_page_streams(&doc);
    let total = all_streams.len();

    if total == 0 {
        return Err(anyhow!("No pages found in {}", input_file));
    }
    if start > total {
        return Err(anyhow!(
            "Start page {} exceeds total pages {}",
            start,
            total
        ));
    }

    let actual_end = end.min(total);
    let selected: Vec<Vec<u8>> = all_streams[(start - 1)..actual_end].to_vec();

    let layout = crate::pdf_generator::PageLayout::portrait();
    assemble_merged_pdf(output_file, &selected, "Helvetica", &layout)?;
    println!(
        "[split] Extracted pages {}-{} ({} pages) from {} into {}",
        start,
        actual_end,
        selected.len(),
        input_file,
        output_file
    );
    Ok(())
}

// --- Internal helpers ---

/// Extract raw content stream data from each Stream object in a PdfDocument.
/// Each stream that looks like a content stream (contains text operators) becomes one "page".
pub(super) fn extract_page_streams(doc: &crate::pdf::PdfDocument) -> Vec<Vec<u8>> {
    let mut streams = Vec::new();

    // Prefer structural extraction: traverse /Type /Page dictionaries and follow /Contents refs.
    let mut page_ids: Vec<u32> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let crate::pdf::PdfObject::Dictionary(dict) = obj
                && let Some(crate::pdf::PdfValue::Object(crate::pdf::PdfObject::String(kind))) =
                    dict.get("Type")
                && kind == "/Page"
            {
                return Some(*id);
            }
            None
        })
        .collect();
    page_ids.sort_unstable();

    for page_id in page_ids {
        if let Some(crate::pdf::PdfObject::Dictionary(dict)) = doc.objects.get(&page_id)
            && let Some(crate::pdf::PdfValue::Object(crate::pdf::PdfObject::String(
                contents_id_raw,
            ))) = dict.get("Contents")
            && let Ok(contents_id) = contents_id_raw.parse::<u32>()
            && let Some(crate::pdf::PdfObject::Stream { data, .. }) = doc.objects.get(&contents_id)
        {
            streams.push(decompress_if_needed(data));
        }
    }

    // Fallback for malformed/simple PDFs where /Page dictionaries are not parsed as expected.
    if !streams.is_empty() {
        return streams;
    }

    let mut sorted_ids: Vec<&u32> = doc.objects.keys().collect();
    sorted_ids.sort();
    for id in sorted_ids {
        if let crate::pdf::PdfObject::Stream { data, .. } = &doc.objects[id] {
            let decompressed = decompress_if_needed(data);
            let content = String::from_utf8_lossy(&decompressed);
            if content.contains("Tj") || content.contains("TJ") || content.contains("BT") {
                streams.push(decompressed);
            }
        }
    }

    streams
}

pub(super) fn decompress_if_needed(data: &[u8]) -> Vec<u8> {
    // Valid zlib header: CMF=0x78 and (CMF*256 + FLG) % 31 == 0
    if data.len() > 2
        && data[0] == 0x78
        && ((data[0] as u16) * 256 + (data[1] as u16)).is_multiple_of(31)
    {
        match crate::compression::decompress_deflate(data) {
            Ok(d) => d,
            Err(_) => data.to_vec(),
        }
    } else {
        data.to_vec()
    }
}

/// Build page content streams from elements via in-memory generation (no /tmp).
pub(super) fn build_page_streams(
    elements: &[crate::elements::Element],
    base_font_size: f32,
    _show_page_numbers: bool,
    layout: crate::pdf_generator::PageLayout,
    image_base_dir: Option<std::path::PathBuf>,
) -> Result<Vec<Vec<u8>>> {
    let bytes = crate::pdf_generator::generate_pdf_bytes_internal_with_base(
        elements,
        "Helvetica",
        base_font_size,
        layout,
        None,
        false,
        None,
        image_base_dir,
    )?;
    let doc = crate::pdf::PdfDocument::load_from_bytes(&bytes)?;
    let streams = extract_page_streams(&doc);
    if streams.is_empty() {
        return Err(anyhow!("No page content streams produced from elements"));
    }
    Ok(streams)
}

/// Assemble a merged PDF from raw page content streams
fn assemble_merged_pdf(
    filename: &str,
    page_streams: &[Vec<u8>],
    font: &str,
    layout: &crate::pdf_generator::PageLayout,
) -> Result<()> {
    let metadata = metadata::PdfMetadata::default();
    metadata::assemble_pdf_with_metadata(filename, page_streams, font, layout, &metadata)
}

/// Rotate pages in a PDF. Creates a new PDF with /Rotate applied to each page.
///
/// `rotation` must be 0, 90, 180, or 270.
pub fn rotate_pdf(input_file: &str, output_file: &str, rotation: u32) -> Result<()> {
    if rotation != 0 && rotation != 90 && rotation != 180 && rotation != 270 {
        return Err(anyhow!(
            "Invalid rotation: {}. Must be 0, 90, 180, or 270.",
            rotation
        ));
    }

    let doc = crate::pdf::PdfDocument::load_from_file(input_file)?;
    let all_streams = extract_page_streams(&doc);

    if all_streams.is_empty() {
        return Err(anyhow!("No pages found in {}", input_file));
    }

    let layout = crate::pdf_generator::PageLayout::portrait();
    assemble_rotated_pdf(output_file, &all_streams, "Helvetica", &layout, rotation)?;
    println!(
        "[rotate] Rotated {} pages by {}° in {}",
        all_streams.len(),
        rotation,
        output_file
    );
    Ok(())
}

/// Assemble PDF with /Rotate on each page
fn assemble_rotated_pdf(
    filename: &str,
    page_streams: &[Vec<u8>],
    font: &str,
    layout: &crate::pdf_generator::PageLayout,
    rotation: u32,
) -> Result<()> {
    let mut generator = crate::pdf_generator::PdfGenerator::new();
    let mut page_ids = Vec::new();
    let pages_obj_id = (page_streams.len() as u32) * 3 + 1;

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
             /Rotate {}\n\
             /Contents {} 0 R\n\
             /Resources << /Font << /F1 {} 0 R >> >>\n\
             >>\n",
            pages_obj_id, layout.width, layout.height, rotation, content_id, font_id
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

/// Create a PDF page with multiple images placed at specified positions
pub fn create_pdf_with_images(
    output_file: &str,
    images: &[(String, f32, f32, f32, f32)], // (path, x, y, width, height)
) -> Result<()> {
    if images.is_empty() {
        return Err(anyhow!("No images provided"));
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
        return Err(anyhow!("No pages found in {}", input_file));
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
fn build_watermark_stream(
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
        return Err(anyhow!("No pages found in {}", input_file));
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
        return Err(anyhow!("No pages found in {}", input_file));
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
fn build_text_watermark_stream(
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
fn build_image_watermark_stream(
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
pub fn reorder_pages(input_file: &str, output_file: &str, page_order: &[usize]) -> Result<()> {
    if page_order.is_empty() {
        return Err(anyhow!("Page order list is empty"));
    }

    let doc = crate::pdf::PdfDocument::load_from_file(input_file)?;
    let all_streams = extract_page_streams(&doc);
    let total = all_streams.len();

    if total == 0 {
        return Err(anyhow!("No pages found in {}", input_file));
    }

    // Validate all page numbers
    for &p in page_order {
        if p == 0 || p > total {
            return Err(anyhow!(
                "Invalid page number {} (document has {} pages)",
                p,
                total
            ));
        }
    }

    let reordered: Vec<Vec<u8>> = page_order
        .iter()
        .map(|&p| all_streams[p - 1].clone())
        .collect();

    let layout = crate::pdf_generator::PageLayout::portrait();
    assemble_merged_pdf(output_file, &reordered, "Helvetica", &layout)?;
    println!(
        "[reorder] Reordered {} pages from {} into {}",
        reordered.len(),
        input_file,
        output_file
    );
    Ok(())
}

pub(crate) fn escape_pdf_meta(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

pub(super) fn extract_pdf_dict_value(dict: &str, key: &str) -> Option<String> {
    // Search for key as a standalone token (followed by whitespace or end)
    let pos = dict
        .match_indices(key)
        .find(|(i, _)| {
            let end = i + key.len();
            end == dict.len()
                || dict[end..]
                    .starts_with(|c: char| c.is_whitespace() || c == '(' || c == '<' || c == '[')
        })
        .map(|(i, _)| i)?;
    let after = dict[pos + key.len()..].trim_start();
    if after.starts_with('(') {
        let end = after.find(')')?;
        Some(after[1..end].to_string())
    } else if after.starts_with('<') && !after.starts_with("<<") {
        let end = after.find('>')?;
        Some(after[1..end].to_string())
    } else if after.starts_with('[') {
        let end = after.find(']')?;
        Some(after[..=end].to_string())
    } else if let Some(name_after) = after.strip_prefix('/') {
        // PDF name: /Name
        let end = name_after
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>' || c == '[')
            .unwrap_or(name_after.len());
        Some(name_after[..end].to_string())
    } else {
        let end = after
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(after.len());
        Some(after[..end].to_string())
    }
}

/// Extract embedded images from a PDF and save them to an output directory.
///
/// Returns a list of saved file paths. Currently supports:
/// - JPEG images (`/Filter /DCTDecode`)
/// - Raw pixel streams saved as `.bin` for manual inspection
///
/// # Arguments
///
/// * `input_path` - Path to the input PDF file
/// * `output_dir` - Directory where extracted images will be saved
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_metadata_escape() {
        assert_eq!(escape_pdf_meta("hello (world)"), "hello \\(world\\)");
        assert_eq!(escape_pdf_meta("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn test_split_invalid_range() {
        let result = split_pdf("nonexistent.pdf", "out.pdf", 0, 5);
        assert!(result.is_err());
        let result = split_pdf("nonexistent.pdf", "out.pdf", 5, 3);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_empty_input() {
        let result = merge_pdfs(&[], "out.pdf");
        assert!(result.is_err());
    }

    #[test]
    fn test_rotate_invalid_angle() {
        let result = rotate_pdf("nonexistent.pdf", "out.pdf", 45);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid rotation"));
    }

    #[test]
    fn test_rotate_valid_angles() {
        // These will fail on file-not-found, not on validation
        for angle in [0, 90, 180, 270] {
            let result = rotate_pdf("nonexistent.pdf", "out.pdf", angle);
            assert!(result.is_err());
            assert!(!result.unwrap_err().to_string().contains("Invalid rotation"));
        }
    }

    #[test]
    fn test_create_pdf_with_images_empty() {
        let result = create_pdf_with_images("out.pdf", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No images"));
    }

    #[test]
    fn test_reorder_empty() {
        let result = reorder_pages("nonexistent.pdf", "out.pdf", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_build_watermark_stream() {
        let layout = crate::pdf_generator::PageLayout::portrait();
        let stream = build_watermark_stream("DRAFT", 48.0, 0.3, &layout);
        let content = String::from_utf8_lossy(&stream);
        assert!(content.contains("(DRAFT) Tj"));
        assert!(content.contains("0.7071")); // cos(45)
        assert!(content.contains("q\n")); // save state
        assert!(content.contains("Q\n")); // restore state
    }

    #[test]
    fn test_color_constructors() {
        let black = crate::pdf_generator::Color::black();
        assert_eq!(black.r, 0.0);
        assert_eq!(black.g, 0.0);
        assert_eq!(black.b, 0.0);

        let red = crate::pdf_generator::Color::red();
        assert_eq!(red.r, 1.0);

        let custom = crate::pdf_generator::Color::rgb(0.2, 0.4, 0.6);
        assert_eq!(custom.r, 0.2);
        assert_eq!(custom.g, 0.4);
        assert_eq!(custom.b, 0.6);
    }

    #[test]
    fn test_build_text_watermark_positions() {
        let layout = crate::pdf_generator::PageLayout::portrait();

        // Test different positions
        let center_stream =
            build_text_watermark_stream("TEST", 24.0, 0.5, &layout, WatermarkPosition::Center);
        assert!(String::from_utf8_lossy(&center_stream).contains("(TEST) Tj"));

        let diagonal_stream =
            build_text_watermark_stream("DRAFT", 48.0, 0.3, &layout, WatermarkPosition::Diagonal);
        let content = String::from_utf8_lossy(&diagonal_stream);
        assert!(content.contains("(DRAFT) Tj"));
        assert!(content.contains("0.707")); // cos(45°)
    }

    #[test]
    fn test_watermark_position_variants() {
        // Test that all watermark position variants work
        let layout = crate::pdf_generator::PageLayout::portrait();

        for position in [
            WatermarkPosition::Center,
            WatermarkPosition::TopLeft,
            WatermarkPosition::TopRight,
            WatermarkPosition::BottomLeft,
            WatermarkPosition::BottomRight,
            WatermarkPosition::Diagonal,
        ] {
            let stream = build_text_watermark_stream("TEST", 24.0, 0.5, &layout, position);
            assert!(!stream.is_empty());
        }
    }

    #[test]
    fn test_image_watermark_stream() {
        let layout = crate::pdf_generator::PageLayout::portrait();
        let image_info = crate::image::ImageInfo {
            format: crate::image::ImageFormat::Jpeg,
            width: 800,
            height: 600,
            data: vec![],
            bits_per_component: 8,
            color_components: 3,
            alt_text: None,
        };

        let result =
            build_image_watermark_stream(&image_info, 0.5, &layout, WatermarkPosition::Center);
        assert!(result.is_ok());

        let stream = result.unwrap();
        let content = String::from_utf8_lossy(&stream);
        assert!(content.contains("/Im1 Do"));
        assert!(content.contains("q\n"));
        assert!(content.contains("Q\n"));
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn escape_pdf_meta_roundtrip(s in ".*") {
            let escaped = escape_pdf_meta(&s);
            // After escaping, certain patterns should be consistent
            // Escaped parens should be present
            for (_, c) in s.chars().enumerate() {
                match c {
                    '(' | ')' => {
                        // Should be escaped
                        assert!(escaped.contains(&format!(r"\{}", c)));
                    }
                    '\\' => {
                        // Should be escaped
                        assert!(escaped.contains(r"\\"));
                    }
                    _ => {}
                }
            }
        }
    }
}
