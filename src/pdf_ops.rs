use anyhow::{anyhow, Result};
use std::fs;

/// Merge multiple PDF files into a single output PDF.
///
/// Strategy: Re-parse each input PDF's page content streams,
/// then reassemble them into a single PDF with a unified page tree.
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

/// Split a PDF by extracting a range of pages into a new PDF.
///
/// `start` and `end` are 1-indexed inclusive page numbers.
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

/// Document metadata
#[derive(Debug, Clone, Default)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
}

impl PdfMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a PDF Info dictionary string
    fn to_info_dict(&self) -> String {
        let mut entries = Vec::new();
        if let Some(ref t) = self.title {
            entries.push(format!("/Title ({})", escape_pdf_meta(t)));
        }
        if let Some(ref a) = self.author {
            entries.push(format!("/Author ({})", escape_pdf_meta(a)));
        }
        if let Some(ref s) = self.subject {
            entries.push(format!("/Subject ({})", escape_pdf_meta(s)));
        }
        if let Some(ref k) = self.keywords {
            entries.push(format!("/Keywords ({})", escape_pdf_meta(k)));
        }
        if let Some(ref c) = self.creator {
            entries.push(format!("/Creator ({})", escape_pdf_meta(c)));
        }
        entries.push("/Producer (pdf-cli)".to_string());
        format!("<<\n{}\n>>\n", entries.join("\n"))
    }
}

/// Create a PDF from markdown with metadata embedded
pub fn create_pdf_with_metadata(
    markdown_file: &str,
    output_file: &str,
    font: &str,
    font_size: f32,
    orientation: crate::pdf_generator::PageOrientation,
    metadata: &PdfMetadata,
) -> Result<()> {
    let content = fs::read_to_string(markdown_file)?;
    let elements = crate::elements::parse_markdown(&content);
    let layout = crate::pdf_generator::PageLayout::from_orientation(orientation);

    create_pdf_elements_with_metadata(output_file, &elements, font, font_size, layout, metadata)
}

/// Low-level: create PDF from elements with metadata
pub fn create_pdf_elements_with_metadata(
    filename: &str,
    elements: &[crate::elements::Element],
    font: &str,
    base_font_size: f32,
    layout: crate::pdf_generator::PageLayout,
    metadata: &PdfMetadata,
) -> Result<()> {
    use crate::elements::Element;

    let show_page_numbers = true;
    let page_streams = build_page_streams(elements, base_font_size, show_page_numbers, layout);

    assemble_pdf_with_metadata(filename, &page_streams, font, &layout, metadata)?;
    Ok(())
}

// --- Internal helpers ---

/// Extract raw content stream data from each Stream object in a PdfDocument.
/// Each stream that looks like a content stream (contains text operators) becomes one "page".
fn extract_page_streams(doc: &crate::pdf::PdfDocument) -> Vec<Vec<u8>> {
    let mut streams = Vec::new();
    let mut sorted_ids: Vec<&u32> = doc.objects.keys().collect();
    sorted_ids.sort();

    for id in sorted_ids {
        if let crate::pdf::PdfObject::Stream { data, .. } = &doc.objects[id] {
            let decompressed = decompress_if_needed(data);
            let content = String::from_utf8_lossy(&decompressed);
            // Heuristic: content streams contain text operators like Tj, TJ, BT, ET
            if content.contains("Tj") || content.contains("TJ") || content.contains("BT") {
                streams.push(decompressed);
            }
        }
    }
    streams
}

fn decompress_if_needed(data: &[u8]) -> Vec<u8> {
    if data.len() > 2 && data[0] == 0x78 && (data[1] == 0x9C || data[1] == 0xDA) {
        match crate::compression::decompress_deflate(data) {
            Ok(d) => d,
            Err(_) => data.to_vec(),
        }
    } else {
        data.to_vec()
    }
}

/// Build page content streams from elements (reuses ContentStreamBuilder logic)
fn build_page_streams(
    elements: &[crate::elements::Element],
    base_font_size: f32,
    _show_page_numbers: bool,
    _layout: crate::pdf_generator::PageLayout,
) -> Vec<Vec<u8>> {
    // Delegate to the existing public API by creating a temp file, then reading it back.
    // This is not ideal but avoids duplicating ContentStreamBuilder.
    // A better approach: refactor ContentStreamBuilder to be public. For now, use the
    // element-to-PDF pipeline and re-extract streams.
    //
    // Actually, let's just call create_pdf_from_elements_with_layout to a temp file,
    // then load it back and extract streams.
    let tmp = format!(
        "/tmp/pdf_cli_build_{}.pdf",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    if crate::pdf_generator::create_pdf_from_elements_with_layout(
        &tmp,
        elements,
        "Helvetica",
        base_font_size,
        _layout,
    )
    .is_ok()
    {
        if let Ok(doc) = crate::pdf::PdfDocument::load_from_file(&tmp) {
            let streams = extract_page_streams(&doc);
            let _ = fs::remove_file(&tmp);
            return streams;
        }
        let _ = fs::remove_file(&tmp);
    }
    Vec::new()
}

/// Assemble a merged PDF from raw page content streams
fn assemble_merged_pdf(
    filename: &str,
    page_streams: &[Vec<u8>],
    font: &str,
    layout: &crate::pdf_generator::PageLayout,
) -> Result<()> {
    let metadata = PdfMetadata::default();
    assemble_pdf_with_metadata(filename, page_streams, font, layout, &metadata)
}

/// Assemble PDF with optional metadata Info dictionary
fn assemble_pdf_with_metadata(
    filename: &str,
    page_streams: &[Vec<u8>],
    font: &str,
    layout: &crate::pdf_generator::PageLayout,
    metadata: &PdfMetadata,
) -> Result<()> {
    let mut generator = crate::pdf_generator::PdfGenerator::new();
    let mut page_ids = Vec::new();

    let has_metadata = metadata.title.is_some()
        || metadata.author.is_some()
        || metadata.subject.is_some()
        || metadata.keywords.is_some()
        || metadata.creator.is_some();

    // Object layout: for each page: content_stream, page, font (3 per page)
    // Then: pages, info (optional), catalog
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
             /Contents {} 0 R\n\
             /Resources << /Font << /F1 {} 0 R >> >>\n\
             >>\n",
            pages_obj_id, layout.width, layout.height, content_id, font_id
        );
        let page_id = generator.add_object(page_dict);
        page_ids.push(page_id);

        let font_dict = format!(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
            font
        );
        generator.add_object(font_dict);
    }

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let pages_dict = format!(
        "<< /Type /Pages\n\
         /Kids [{}]\n\
         /Count {}\n\
         >>\n",
        kids.join(" "),
        page_ids.len()
    );
    let actual_pages_id = generator.add_object(pages_dict);
    assert_eq!(actual_pages_id, pages_obj_id);

    // Info dictionary (optional)
    let info_id = if has_metadata {
        Some(generator.add_object(metadata.to_info_dict()))
    } else {
        // Always add producer
        let default_meta = PdfMetadata::default();
        Some(generator.add_object(default_meta.to_info_dict()))
    };

    // Catalog
    let catalog_dict = format!(
        "<< /Type /Catalog\n\
         /Pages {} 0 R\n\
         >>\n",
        actual_pages_id
    );
    generator.add_object(catalog_dict);

    // Generate with info reference
    let pdf_data = if let Some(info) = info_id {
        generate_with_info(&generator, info)
    } else {
        generator.generate()
    };

    let mut file = std::fs::File::create(filename)?;
    std::io::Write::write_all(&mut file, &pdf_data)?;
    Ok(())
}

/// Generate PDF bytes with an /Info reference in the trailer
fn generate_with_info(generator: &crate::pdf_generator::PdfGenerator, info_id: u32) -> Vec<u8> {
    let mut pdf = Vec::new();

    pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    let mut offsets = Vec::new();
    let mut current_offset = pdf.len() as u32;

    for obj in &generator.objects {
        offsets.push(current_offset);
        let obj_header = format!("{} {} obj\n", obj.id, obj.generation);
        pdf.extend_from_slice(obj_header.as_bytes());
        pdf.extend_from_slice(obj.content.as_bytes());

        if obj.is_stream {
            if let Some(data) = &obj.stream_data {
                pdf.extend_from_slice(b"stream\n");
                pdf.extend_from_slice(data);
                pdf.extend_from_slice(b"\nendstream\n");
            }
        }

        pdf.extend_from_slice(b"endobj\n");
        current_offset = pdf.len() as u32;
    }

    let xref_offset = pdf.len() as u32;
    pdf.extend_from_slice(format!("xref\n0 {}\n", generator.objects.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");

    for offset in offsets {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
    }

    pdf.extend_from_slice(b"trailer\n");
    pdf.extend_from_slice(b"<<\n");
    pdf.extend_from_slice(format!("/Size {}\n", generator.objects.len() + 1).as_bytes());
    if !generator.objects.is_empty() {
        pdf.extend_from_slice(format!("/Root {} 0 R\n", generator.objects.len()).as_bytes());
    }
    pdf.extend_from_slice(format!("/Info {} 0 R\n", info_id).as_bytes());
    pdf.extend_from_slice(b">>\n");
    pdf.extend_from_slice(b"startxref\n");
    pdf.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    pdf.extend_from_slice(b"%%EOF\n");

    pdf
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
        let font_dict = format!(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
            font
        );
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

    let catalog_dict = format!(
        "<< /Type /Catalog\n/Pages {} 0 R\n>>\n",
        actual_pages_id
    );
    generator.add_object(catalog_dict);

    let pdf_data = generator.generate();
    let mut file = std::fs::File::create(filename)?;
    std::io::Write::write_all(&mut file, &pdf_data)?;
    Ok(())
}

/// A text annotation to be placed on a PDF page
#[derive(Debug, Clone)]
pub struct TextAnnotation {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub content: String,
    pub title: String,
}

/// A link annotation (clickable URL region)
#[derive(Debug, Clone)]
pub struct LinkAnnotation {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub url: String,
}

/// Create a single-page PDF with text annotations
pub fn create_pdf_with_annotations(
    output_file: &str,
    text: &str,
    annotations: &[TextAnnotation],
    links: &[LinkAnnotation],
) -> Result<()> {
    let elements = crate::elements::parse_markdown(text);
    let layout = crate::pdf_generator::PageLayout::portrait();

    // Build page content
    let page_streams = build_page_streams(&elements, 12.0, true, layout);
    if page_streams.is_empty() {
        return Err(anyhow!("No page content generated"));
    }

    let mut generator = crate::pdf_generator::PdfGenerator::new();

    // Build annotation objects first, collect their IDs
    let mut annot_ids: Vec<u32> = Vec::new();

    for annot in annotations {
        let annot_dict = format!(
            "<< /Type /Annot\n\
             /Subtype /Text\n\
             /Rect [{} {} {} {}]\n\
             /Contents ({})\n\
             /T ({})\n\
             /Open false\n\
             >>\n",
            annot.x,
            annot.y,
            annot.x + annot.width,
            annot.y + annot.height,
            escape_pdf_meta(&annot.content),
            escape_pdf_meta(&annot.title),
        );
        annot_ids.push(generator.add_object(annot_dict));
    }

    for link in links {
        let link_dict = format!(
            "<< /Type /Annot\n\
             /Subtype /Link\n\
             /Rect [{} {} {} {}]\n\
             /Border [0 0 0]\n\
             /A << /Type /Action\n/S /URI\n/URI ({}) >>\n\
             >>\n",
            link.x,
            link.y,
            link.x + link.width,
            link.y + link.height,
            escape_pdf_meta(&link.url),
        );
        annot_ids.push(generator.add_object(link_dict));
    }

    let annot_offset = annot_ids.len() as u32;

    // Now add page content streams and pages
    // pages_obj_id = annot_offset + page_streams.len() * 3 + 1
    let pages_obj_id = annot_offset + (page_streams.len() as u32) * 3 + 1;

    let mut page_ids = Vec::new();
    for (i, page_stream) in page_streams.iter().enumerate() {
        let content_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", page_stream.len()),
            page_stream.clone(),
        );
        let font_id = content_id + 2;

        // Only first page gets annotations
        let annots_str = if i == 0 && !annot_ids.is_empty() {
            let refs: Vec<String> = annot_ids.iter().map(|id| format!("{} 0 R", id)).collect();
            format!("/Annots [{}]\n", refs.join(" "))
        } else {
            String::new()
        };

        let page_dict = format!(
            "<< /Type /Page\n\
             /Parent {} 0 R\n\
             /MediaBox [0 0 {} {}]\n\
             /Contents {} 0 R\n\
             {}\
             /Resources << /Font << /F1 {} 0 R >> >>\n\
             >>\n",
            pages_obj_id, layout.width, layout.height, content_id, annots_str, font_id
        );
        let page_id = generator.add_object(page_dict);
        page_ids.push(page_id);

        let font_dict = format!(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n"
        );
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

    let catalog_dict = format!(
        "<< /Type /Catalog\n/Pages {} 0 R\n>>\n",
        actual_pages_id
    );
    generator.add_object(catalog_dict);

    let pdf_data = generator.generate();
    let mut file = std::fs::File::create(output_file)?;
    std::io::Write::write_all(&mut file, &pdf_data)?;
    println!(
        "[annotate] Created {} with {} text annotations, {} link annotations",
        output_file,
        annotations.len(),
        links.len()
    );
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

    // Create image XObjects
    for (i, (path, _, _, _, _)) in images.iter().enumerate() {
        let info = crate::image::load_image(path)?;
        if info.format != crate::image::ImageFormat::Jpeg {
            return Err(anyhow!("Only JPEG images supported. Got {:?} for {}", info.format, path));
        }
        let name = format!("Im{}", i + 1);
        let image_id = crate::image::create_jpeg_image_object(
            &mut generator,
            info.data,
            info.width,
            info.height,
        );
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

    let content_id = generator.add_stream_object(
        format!("<< /Length {} >>\n", content.len()),
        content,
    );

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

    let pages_dict = format!(
        "<< /Type /Pages\n/Kids [{} 0 R]\n/Count 1\n>>\n",
        page_id
    );
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
fn build_watermark_stream(text: &str, font_size: f32, opacity: f32, layout: &crate::pdf_generator::PageLayout) -> Vec<u8> {
    let escaped = escape_pdf_meta(text);
    // Center of page
    let cx = layout.width / 2.0;
    let cy = layout.height / 2.0;
    // 45° rotation matrix: cos(45)=0.707, sin(45)=0.707
    let cos45: f32 = 0.7071;
    let sin45: f32 = 0.7071;

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
            cos45, sin45, -sin45, cos45, cx - 100.0, cy - 50.0
        )
        .as_bytes(),
    );
    stream.extend_from_slice(format!("({}) Tj\n", escaped).as_bytes());
    stream.extend_from_slice(b"ET\n");
    stream.extend_from_slice(b"Q\n");
    stream
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

fn escape_pdf_meta(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_metadata_info_dict() {
        let meta = PdfMetadata {
            title: Some("Test Title".into()),
            author: Some("Test Author".into()),
            subject: None,
            keywords: None,
            creator: None,
        };
        let dict = meta.to_info_dict();
        assert!(dict.contains("/Title (Test Title)"));
        assert!(dict.contains("/Author (Test Author)"));
        assert!(dict.contains("/Producer (pdf-cli)"));
        assert!(!dict.contains("/Subject"));
    }

    #[test]
    fn test_pdf_metadata_escape() {
        assert_eq!(escape_pdf_meta("hello (world)"), "hello \\(world\\)");
        assert_eq!(escape_pdf_meta("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn test_pdf_metadata_default() {
        let meta = PdfMetadata::new();
        assert!(meta.title.is_none());
        assert!(meta.author.is_none());
        let dict = meta.to_info_dict();
        assert!(dict.contains("/Producer (pdf-cli)"));
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
    fn test_text_annotation_struct() {
        let annot = TextAnnotation {
            x: 100.0,
            y: 700.0,
            width: 200.0,
            height: 20.0,
            content: "A note".into(),
            title: "Author".into(),
        };
        assert_eq!(annot.content, "A note");
        assert_eq!(annot.x, 100.0);
    }

    #[test]
    fn test_link_annotation_struct() {
        let link = LinkAnnotation {
            x: 72.0,
            y: 500.0,
            width: 100.0,
            height: 15.0,
            url: "https://example.com".into(),
        };
        assert_eq!(link.url, "https://example.com");
    }
}
