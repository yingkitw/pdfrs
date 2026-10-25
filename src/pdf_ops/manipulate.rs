//! Page-level PDF manipulation: merge, split, rotate, reorder.
use super::{assemble_merged_pdf, extract_page_streams};
use crate::error::{PdfError, Result};

pub fn merge_pdfs(input_files: &[&str], output_file: &str) -> Result<()> {
    if input_files.is_empty() {
        return Err(PdfError::InvalidInput(
            "No input files provided for merge".into(),
        ));
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
        return Err(PdfError::Parse(
            "No page content found in any input file".into(),
        ));
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

/// Merge multiple PDFs from raw byte slices (no filesystem access).
///
/// Each entry in `pdf_bytes` is a complete PDF file. Returns the merged PDF bytes.
pub fn merge_pdfs_from_bytes(pdf_bytes: &[Vec<u8>]) -> Result<Vec<u8>> {
    if pdf_bytes.is_empty() {
        return Err(PdfError::InvalidInput(
            "No input PDFs provided for merge".into(),
        ));
    }
    let mut all_page_streams: Vec<Vec<u8>> = Vec::new();
    for (i, bytes) in pdf_bytes.iter().enumerate() {
        let doc = crate::pdf::PdfDocument::load_from_bytes(bytes)?;
        let streams = extract_page_streams(&doc);
        if streams.is_empty() {
            eprintln!("[merge] Warning: no page streams found in PDF #{i}");
        }
        all_page_streams.extend(streams);
    }
    if all_page_streams.is_empty() {
        return Err(PdfError::Parse(
            "No page content found in any input PDF".into(),
        ));
    }
    let layout = crate::pdf_generator::PageLayout::portrait();
    assemble_merged_pdf_bytes(&all_page_streams, "Helvetica", &layout)
}

/// Split a PDF from raw bytes into individual page PDFs (one byte vec per page).
pub fn split_pdf_from_bytes(pdf_bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let doc = crate::pdf::PdfDocument::load_from_bytes(pdf_bytes)?;
    let all_streams = extract_page_streams(&doc);
    if all_streams.is_empty() {
        return Err(PdfError::Parse("No pages found in input PDF".into()));
    }
    let layout = crate::pdf_generator::PageLayout::portrait();
    let mut results = Vec::with_capacity(all_streams.len());
    for stream in &all_streams {
        let pdf = assemble_merged_pdf_bytes(std::slice::from_ref(stream), "Helvetica", &layout)?;
        results.push(pdf);
    }
    Ok(results)
}

/// Assemble a merged PDF from raw page content streams, returning bytes.
fn assemble_merged_pdf_bytes(
    page_streams: &[Vec<u8>],
    font: &str,
    layout: &crate::pdf_generator::PageLayout,
) -> Result<Vec<u8>> {
    // Build the PDF in memory using the same structure as assemble_merged_pdf
    // but writing to a Vec instead of a file.
    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    let mut offsets: Vec<(u32, u64)> = Vec::new();
    let mut next_id = 1u32;

    // Font object
    let font_id = next_id;
    offsets.push((font_id, pdf.len() as u64));
    pdf.extend_from_slice(
        format!(
            "{} 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /{} >>\nendobj\n",
            font_id, font
        )
        .as_bytes(),
    );
    next_id += 1;

    // Pages object
    let pages_id = next_id;
    let pages_offset = pdf.len() as u64;
    next_id += 1;

    // Page objects
    let mut page_ids = Vec::new();
    for stream in page_streams {
        let content_id = next_id;
        offsets.push((content_id, pdf.len() as u64));
        pdf.extend_from_slice(
            format!(
                "{} 0 obj\n<< /Length {} >>\nstream\n",
                content_id,
                stream.len()
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(stream);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");
        next_id += 1;

        let page_id = next_id;
        offsets.push((page_id, pdf.len() as u64));
        pdf.extend_from_slice(
            format!(
                "{} 0 obj\n<< /Type /Page /Parent {} 0 R /MediaBox [0 0 {} {}] /Contents {} 0 R /Resources << /Font << /F1 {} 0 R >> >> >>\nendobj\n",
                page_id,
                pages_id,
                layout.width,
                layout.height,
                content_id,
                font_id
            )
            .as_bytes(),
        );
        page_ids.push(page_id);
        next_id += 1;
    }

    // Write pages object
    offsets.push((pages_id, pages_offset));
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    pdf.extend_from_slice(
        format!(
            "{} 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n",
            pages_id,
            kids.join(" "),
            page_ids.len()
        )
        .as_bytes(),
    );

    // Catalog
    let catalog_id = next_id;
    offsets.push((catalog_id, pdf.len() as u64));
    pdf.extend_from_slice(
        format!(
            "{} 0 obj\n<< /Type /Catalog /Pages {} 0 R >>\nendobj\n",
            catalog_id, pages_id
        )
        .as_bytes(),
    );

    // xref
    let xref_offset = pdf.len() as u64;
    pdf.extend_from_slice(b"xref\n");
    pdf.extend_from_slice(format!("0 {}\n", next_id).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for (_, offset) in &offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }

    // Trailer
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root {} 0 R >>\nstartxref\n{}\n%%EOF\n",
            next_id, catalog_id, xref_offset
        )
        .as_bytes(),
    );

    Ok(pdf)
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
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn merge_pdfs_sequential(
    documents: &[crate::pdf::PdfDocument],
    output_file: &str,
) -> Result<()> {
    if documents.is_empty() {
        return Err(PdfError::InvalidInput(
            "No documents provided for merge".into(),
        ));
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
        return Err(PdfError::Parse(
            "No page content found in any document".into(),
        ));
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
        return Err(PdfError::InvalidInput(format!(
            "Invalid page range: start={} end={} (1-indexed, inclusive)",
            start, end
        )));
    }

    let doc = crate::pdf::PdfDocument::load_from_file(input_file)?;
    let all_streams = extract_page_streams(&doc);
    let total = all_streams.len();

    if total == 0 {
        return Err(PdfError::Parse(format!("No pages found in {}", input_file)));
    }
    if start > total {
        return Err(PdfError::PageNotFound(start));
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

/// Rotate pages in a PDF. Creates a new PDF with /Rotate applied to each page.
///
/// `rotation` must be 0, 90, 180, or 270.
pub fn rotate_pdf(input_file: &str, output_file: &str, rotation: u32) -> Result<()> {
    if rotation != 0 && rotation != 90 && rotation != 180 && rotation != 270 {
        return Err(PdfError::InvalidInput(format!(
            "Invalid rotation: {}. Must be 0, 90, 180, or 270.",
            rotation
        )));
    }

    let doc = crate::pdf::PdfDocument::load_from_file(input_file)?;
    let all_streams = extract_page_streams(&doc);

    if all_streams.is_empty() {
        return Err(PdfError::Parse(format!("No pages found in {}", input_file)));
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
pub fn reorder_pages(input_file: &str, output_file: &str, page_order: &[usize]) -> Result<()> {
    if page_order.is_empty() {
        return Err(PdfError::InvalidInput("Page order list is empty".into()));
    }

    let doc = crate::pdf::PdfDocument::load_from_file(input_file)?;
    let all_streams = extract_page_streams(&doc);
    let total = all_streams.len();

    if total == 0 {
        return Err(PdfError::Parse(format!("No pages found in {}", input_file)));
    }

    // Validate all page numbers
    for &p in page_order {
        if p == 0 || p > total {
            return Err(PdfError::PageNotFound(p));
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
