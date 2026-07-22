//! PDF high-level operations module
//!
//! This module provides high-level operations for manipulating PDF documents,
//! including merging, splitting, rotating, watermarking, and annotations.

use anyhow::{anyhow, Result};
use std::fs;
use serde::{Serialize, Deserialize};

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

/// Document metadata.
///
/// Represents standard PDF document metadata fields including title, author,
/// subject, keywords, and creator. Also supports custom metadata fields.
///
/// # Fields
///
/// * `title` - Document title
/// * `author` - Document author
/// * `subject` - Document subject
/// * `keywords` - Document keywords
/// * `creator` - Application that created the document
/// * `custom_fields` - Custom metadata fields as key-value pairs
///
/// # Example
///
/// ```rust
/// use pdfrs::pdf_ops::PdfMetadata;
///
/// let mut metadata = PdfMetadata::new();
/// metadata.title = Some("My Document".to_string());
/// metadata.author = Some("John Doe".to_string());
/// metadata.add_custom_field("Version".to_string(), "1.0".to_string());
/// ```
#[derive(Debug, Clone, Default)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    /// Custom metadata fields (key-value pairs)
    pub custom_fields: std::collections::HashMap<String, String>,
}

impl PdfMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a custom metadata field
    pub fn add_custom_field(&mut self, key: String, value: String) {
        self.custom_fields.insert(key, value);
    }

    /// Get a custom metadata field
    pub fn get_custom_field(&self, key: &str) -> Option<&String> {
        self.custom_fields.get(key)
    }

    /// Remove a custom metadata field
    pub fn remove_custom_field(&mut self, key: &str) -> Option<String> {
        self.custom_fields.remove(key)
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

        // Add custom fields
        for (key, value) in &self.custom_fields {
            // Escape the key as well (though typically keys are simple strings)
            let escaped_key = escape_pdf_meta(key);
            let escaped_value = escape_pdf_meta(value);
            entries.push(format!("/{} ({})", escaped_key, escaped_value));
        }

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
    let image_base = std::path::Path::new(markdown_file)
        .parent()
        .map(|p| p.to_path_buf());

    create_pdf_elements_with_metadata_and_images(
        output_file,
        &elements,
        font,
        font_size,
        layout,
        metadata,
        image_base,
    )
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
    create_pdf_elements_with_metadata_and_images(
        filename,
        elements,
        font,
        base_font_size,
        layout,
        metadata,
        None,
    )
}

fn create_pdf_elements_with_metadata_and_images(
    filename: &str,
    elements: &[crate::elements::Element],
    font: &str,
    base_font_size: f32,
    layout: crate::pdf_generator::PageLayout,
    metadata: &PdfMetadata,
    image_base_dir: Option<std::path::PathBuf>,
) -> Result<()> {
    let show_page_numbers = true;
    let page_streams = build_page_streams(
        elements,
        base_font_size,
        show_page_numbers,
        layout,
        image_base_dir,
    )?;

    assemble_pdf_with_metadata(filename, &page_streams, font, &layout, metadata)?;
    Ok(())
}

// --- Internal helpers ---

/// Extract raw content stream data from each Stream object in a PdfDocument.
/// Each stream that looks like a content stream (contains text operators) becomes one "page".
fn extract_page_streams(doc: &crate::pdf::PdfDocument) -> Vec<Vec<u8>> {
    let mut streams = Vec::new();

    // Prefer structural extraction: traverse /Type /Page dictionaries and follow /Contents refs.
    let mut page_ids: Vec<u32> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let crate::pdf::PdfObject::Dictionary(dict) = obj
                && let Some(crate::pdf::PdfValue::Object(crate::pdf::PdfObject::String(kind))) = dict.get("Type")
                    && kind == "/Page" {
                        return Some(*id);
                    }
            None
        })
        .collect();
    page_ids.sort_unstable();

    for page_id in page_ids {
        if let Some(crate::pdf::PdfObject::Dictionary(dict)) = doc.objects.get(&page_id)
            && let Some(crate::pdf::PdfValue::Object(crate::pdf::PdfObject::String(contents_id_raw))) = dict.get("Contents")
                && let Ok(contents_id) = contents_id_raw.parse::<u32>()
                    && let Some(crate::pdf::PdfObject::Stream { data, .. }) = doc.objects.get(&contents_id) {
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

fn decompress_if_needed(data: &[u8]) -> Vec<u8> {
    // Valid zlib header: CMF=0x78 and (CMF*256 + FLG) % 31 == 0
    if data.len() > 2 && data[0] == 0x78 && ((data[0] as u16) * 256 + (data[1] as u16)).is_multiple_of(31) {
        match crate::compression::decompress_deflate(data) {
            Ok(d) => d,
            Err(_) => data.to_vec(),
        }
    } else {
        data.to_vec()
    }
}

/// Build page content streams from elements via in-memory generation (no /tmp).
fn build_page_streams(
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

    if let Some(info) = info_id {
        generator.info_id = Some(info);
    }
    let pdf_data = generator.generate();

    let mut file = std::fs::File::create(filename)?;
    std::io::Write::write_all(&mut file, &pdf_data)?;
    Ok(())
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

/// Extract metadata from a PDF document
pub fn extract_metadata_from_pdf(doc: &crate::pdf::PdfDocument) -> Result<PdfMetadata> {
    let mut metadata = PdfMetadata::new();

    // Look for the Info dictionary in the trailer
    // For now, we'll do a simple search for metadata-like objects
    for obj in doc.objects.values() {
        if let crate::pdf::PdfObject::Dictionary(data) = obj {
            // Convert dictionary to a string representation for parsing
            let dict_str = dict_to_string(data);
            if dict_str.contains("/Title")
                && let Some(title) = extract_pdf_string_field(&dict_str, "/Title") {
                    metadata.title = Some(title);
                }
            if dict_str.contains("/Author")
                && let Some(author) = extract_pdf_string_field(&dict_str, "/Author") {
                    metadata.author = Some(author);
                }
            if dict_str.contains("/Subject")
                && let Some(subject) = extract_pdf_string_field(&dict_str, "/Subject") {
                    metadata.subject = Some(subject);
                }
            if dict_str.contains("/Keywords")
                && let Some(keywords) = extract_pdf_string_field(&dict_str, "/Keywords") {
                    metadata.keywords = Some(keywords);
                }
            if dict_str.contains("/Creator")
                && let Some(creator) = extract_pdf_string_field(&dict_str, "/Creator") {
                    metadata.creator = Some(creator);
                }
        }
    }

    Ok(metadata)
}

/// Convert a PDF dictionary HashMap to a string representation
fn dict_to_string(dict: &std::collections::HashMap<String, crate::pdf::PdfValue>) -> String {
    let mut parts = Vec::new();
    for (key, value) in dict {
        parts.push(format!("/{} {}", key, value_to_string(value)));
    }
    parts.join(" ")
}

/// Convert a PdfValue to its string representation
fn value_to_string(value: &crate::pdf::PdfValue) -> String {
    match value {
        crate::pdf::PdfValue::Object(obj) => object_to_string(obj),
        crate::pdf::PdfValue::Reference(id, generation) => format!("{} {} R", id, generation),
    }
}

/// Convert a PdfObject to its string representation
fn object_to_string(obj: &crate::pdf::PdfObject) -> String {
    match obj {
        crate::pdf::PdfObject::Dictionary(dict) => {
            let entries: Vec<String> = dict.iter()
                .map(|(k, v)| format!("/{} {}", k, value_to_string(v)))
                .collect();
            format!("<< {} >>", entries.join(" "))
        }
        crate::pdf::PdfObject::Stream { dictionary: _, data: _ } => {
            "<< stream >>".to_string()
        }
        crate::pdf::PdfObject::Array(arr) => {
            let elems: Vec<String> = arr.iter().map(value_to_string).collect();
            format!("[{}]", elems.join(" "))
        }
        crate::pdf::PdfObject::String(s) => format!("({})", escape_pdf_meta(s)),
        crate::pdf::PdfObject::Number(n) => n.to_string(),
        crate::pdf::PdfObject::Boolean(b) => {
            if *b { "true" } else { "false" }.to_string()
        }
        crate::pdf::PdfObject::Null => "null".to_string(),
        crate::pdf::PdfObject::Reference(id, generation) => format!("{} {} R", id, generation),
        crate::pdf::PdfObject::Name(n) => format!("/{}", n),
    }
}

/// Extract a string field value from PDF dictionary content
fn extract_pdf_string_field(content: &str, field: &str) -> Option<String> {
    // Find the field and extract the string value
    // Format: /Field (value) or /Field <value>
    // Look for the field name followed by optional whitespace and opening parenthesis
    let field_pattern_start = format!("{} ", field);
    if let Some(start) = content.find(&field_pattern_start) {
        // Find the opening parenthesis after the field name
        let after_field = &content[start + field_pattern_start.len()..];
        if let Some(paren_start) = after_field.find('(') {
            let value_start = start + field_pattern_start.len() + paren_start + 1;
            // Find the closing parenthesis, handling escaped parentheses
            let mut paren_count = 1;
            let mut value_end = value_start;
            let chars: Vec<char> = content[value_start..].chars().collect();
            let mut i = 0;
            while i < chars.len() && paren_count > 0 {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    // Escaped character, skip it
                    i += 2;
                    continue;
                }
                if chars[i] == '(' {
                    paren_count += 1;
                } else if chars[i] == ')' {
                    paren_count -= 1;
                }
                if paren_count > 0 {
                    value_end = value_start + i + 1;
                }
                i += 1;
            }
            let value = &content[value_start..value_end];
            // Unescape the string
            Some(unescape_pdf_string(value))
        } else {
            None
        }
    } else {
        None
    }
}

/// Unescape a PDF string (handle escape sequences)
fn unescape_pdf_string(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    'b' => result.push('\x08'),
                    'f' => result.push('\x0c'),
                    '(' | ')' | '\\' => result.push(next),
                    '0'..='7' => {
                        // Octal escape sequence (up to 3 digits)
                        let mut octal = String::from(next);
                        if let Some(&c) = chars.peek()
                            && ('0'..='7').contains(&c) {
                                chars.next();
                                octal.push(c);
                                if let Some(&c) = chars.peek()
                                    && ('0'..='7').contains(&c) {
                                        chars.next();
                                        octal.push(c);
                                    }
                            }
                        if let Ok(code) = u8::from_str_radix(&octal, 8) {
                            result.push(code as char);
                        }
                    }
                    _ => result.push(next),
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Merge metadata from two sources, with new_metadata taking precedence
pub fn merge_metadata(base: &PdfMetadata, new_metadata: &PdfMetadata) -> PdfMetadata {
    let mut merged = base.clone();
    if new_metadata.title.is_some() {
        merged.title = new_metadata.title.clone();
    }
    if new_metadata.author.is_some() {
        merged.author = new_metadata.author.clone();
    }
    if new_metadata.subject.is_some() {
        merged.subject = new_metadata.subject.clone();
    }
    if new_metadata.keywords.is_some() {
        merged.keywords = new_metadata.keywords.clone();
    }
    if new_metadata.creator.is_some() {
        merged.creator = new_metadata.creator.clone();
    }
    // Merge custom fields, with new_metadata taking precedence
    for (key, value) in &new_metadata.custom_fields {
        merged.custom_fields.insert(key.clone(), value.clone());
    }
    merged
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

/// A highlight annotation (colored rectangle over text)
#[derive(Debug, Clone)]
pub struct HighlightAnnotation {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color_r: f32,
    pub color_g: f32,
    pub color_b: f32,
}

/// A 3D annotation referencing an embedded U3D stream (PDF 1.6+ / ISO 32000 §13.6).
#[derive(Debug, Clone)]
pub struct ThreeDAnnotation {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Optional tooltip / contents string shown by viewers.
    pub contents: String,
    /// Activate when the page is opened (`/A /PO`).
    pub activate_on_open: bool,
}

impl Default for ThreeDAnnotation {
    fn default() -> Self {
        Self {
            x: 72.0,
            y: 200.0,
            width: 400.0,
            height: 300.0,
            contents: "3D Model".to_string(),
            activate_on_open: true,
        }
    }
}

/// Create a single-page PDF embedding U3D data as a `/Subtype /3D` annotation.
///
/// The U3D bytes are stored in a `/Type /3D` / `/Subtype /U3D` stream referenced by
/// the annotation's `/3DD` entry. Viewers that support U3D (e.g. Adobe Acrobat) can
/// render the model in the annotation rectangle.
pub fn create_pdf_with_3d_annotation(
    output_file: &str,
    page_label: &str,
    u3d_data: &[u8],
    annot: &ThreeDAnnotation,
) -> Result<()> {
    let bytes = create_pdf_with_3d_annotation_bytes(page_label, u3d_data, annot)?;
    fs::write(output_file, bytes)?;
    println!(
        "[3d] Created {} with U3D annotation ({} bytes model)",
        output_file,
        u3d_data.len()
    );
    Ok(())
}

/// In-memory variant of [`create_pdf_with_3d_annotation`].
pub fn create_pdf_with_3d_annotation_bytes(
    page_label: &str,
    u3d_data: &[u8],
    annot: &ThreeDAnnotation,
) -> Result<Vec<u8>> {
    if u3d_data.is_empty() {
        return Err(anyhow!("U3D data must not be empty"));
    }

    let layout = crate::pdf_generator::PageLayout::portrait();
    let mut generator = crate::pdf_generator::PdfGenerator::new();

    // 1. 3D stream (U3D)
    let stream_dict = format!(
        "<< /Type /3D\n/Subtype /U3D\n/Length {} >>\n",
        u3d_data.len()
    );
    let stream_id = generator.add_stream_object(stream_dict, u3d_data.to_vec());

    // 2. 3D annotation
    let activation = if annot.activate_on_open {
        "/3DA << /A /PO /TB true /NP true >>\n"
    } else {
        "/3DA << /A /XA /TB true /NP true >>\n"
    };
    let annot_dict = format!(
        "<< /Type /Annot\n\
         /Subtype /3D\n\
         /Rect [{} {} {} {}]\n\
         /Contents ({})\n\
         /3DD {} 0 R\n\
         {}\
         /F 4\n\
         >>\n",
        annot.x,
        annot.y,
        annot.x + annot.width,
        annot.y + annot.height,
        escape_pdf_meta(&annot.contents),
        stream_id,
        activation,
    );
    let annot_id = generator.add_object(annot_dict);

    // 3. Page content (simple label above the 3D rect)
    let label_y = (annot.y + annot.height + 20.0).min(layout.height - 36.0);
    let page_stream = format!(
        "BT\n/F1 14 Tf\n1 0 0 1 72 {} Tm\n({}) Tj\nET\n",
        label_y,
        escape_pdf_meta(page_label),
    );
    let content_id = generator.add_stream_object(
        format!("<< /Length {} >>\n", page_stream.len()),
        page_stream.into_bytes(),
    );

    // Pre-compute pages/catalog IDs: content, font, page, pages, catalog
    let font_id = content_id + 1;
    let page_id = content_id + 2;
    let pages_id = content_id + 3;

    let page_dict = format!(
        "<< /Type /Page\n\
         /Parent {} 0 R\n\
         /MediaBox [0 0 {} {}]\n\
         /Contents {} 0 R\n\
         /Annots [{} 0 R]\n\
         /Resources << /Font << /F1 {} 0 R >> >>\n\
         >>\n",
        pages_id, layout.width, layout.height, content_id, annot_id, font_id
    );
    let actual_font_id = generator
        .add_object("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n".to_string());
    assert_eq!(actual_font_id, font_id);
    let actual_page_id = generator.add_object(page_dict);
    assert_eq!(actual_page_id, page_id);

    let pages_dict = format!(
        "<< /Type /Pages\n/Kids [{} 0 R]\n/Count 1\n>>\n",
        page_id
    );
    let actual_pages_id = generator.add_object(pages_dict);
    assert_eq!(actual_pages_id, pages_id);
    generator.add_object(format!(
        "<< /Type /Catalog\n/Pages {} 0 R\n>>\n",
        actual_pages_id
    ));

    Ok(generator.generate())
}

/// Returns true if the PDF bytes contain a 3D annotation and a U3D stream.
pub fn pdf_contains_3d_u3d(pdf_bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(pdf_bytes);
    text.contains("/Subtype /3D") && text.contains("/Subtype /U3D") && text.contains("/3DD")
}

/// Create a PDF with text, link, and highlight annotations
pub fn create_pdf_with_all_annotations(
    output_file: &str,
    text: &str,
    annotations: &[TextAnnotation],
    links: &[LinkAnnotation],
    highlights: &[HighlightAnnotation],
) -> Result<()> {
    let elements = crate::elements::parse_markdown(text);
    let layout = crate::pdf_generator::PageLayout::portrait();
    let page_streams = build_page_streams(&elements, 12.0, true, layout, None)?;
    if page_streams.is_empty() {
        return Err(anyhow!("No page content generated"));
    }

    let mut generator = crate::pdf_generator::PdfGenerator::new();
    let mut annot_ids: Vec<u32> = Vec::new();

    for annot in annotations {
        let annot_dict = format!(
            "<< /Type /Annot\n/Subtype /Text\n/Rect [{} {} {} {}]\n/Contents ({})\n/T ({})\n/Open false\n>>\n",
            annot.x, annot.y, annot.x + annot.width, annot.y + annot.height,
            escape_pdf_meta(&annot.content), escape_pdf_meta(&annot.title),
        );
        annot_ids.push(generator.add_object(annot_dict));
    }

    for link in links {
        let link_dict = format!(
            "<< /Type /Annot\n/Subtype /Link\n/Rect [{} {} {} {}]\n/Border [0 0 0]\n/A << /Type /Action\n/S /URI\n/URI ({}) >>\n>>\n",
            link.x, link.y, link.x + link.width, link.y + link.height,
            escape_pdf_meta(&link.url),
        );
        annot_ids.push(generator.add_object(link_dict));
    }

    for hl in highlights {
        let hl_dict = format!(
            "<< /Type /Annot\n/Subtype /Highlight\n/Rect [{} {} {} {}]\n/C [{} {} {}]\n/QuadPoints [{} {} {} {} {} {} {} {}]\n>>\n",
            hl.x, hl.y, hl.x + hl.width, hl.y + hl.height,
            hl.color_r, hl.color_g, hl.color_b,
            hl.x, hl.y + hl.height, hl.x + hl.width, hl.y + hl.height,
            hl.x, hl.y, hl.x + hl.width, hl.y,
        );
        annot_ids.push(generator.add_object(hl_dict));
    }

    let annot_offset = annot_ids.len() as u32;
    let pages_obj_id = annot_offset + (page_streams.len() as u32) * 3 + 1;
    let mut page_ids = Vec::new();

    for (i, page_stream) in page_streams.iter().enumerate() {
        let content_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", page_stream.len()),
            page_stream.clone(),
        );
        let font_id = content_id + 2;
        let annots_str = if i == 0 && !annot_ids.is_empty() {
            let refs: Vec<String> = annot_ids.iter().map(|id| format!("{} 0 R", id)).collect();
            format!("/Annots [{}]\n", refs.join(" "))
        } else {
            String::new()
        };
        let page_dict = format!(
            "<< /Type /Page\n/Parent {} 0 R\n/MediaBox [0 0 {} {}]\n/Contents {} 0 R\n{}/Resources << /Font << /F1 {} 0 R >> >>\n>>\n",
            pages_obj_id, layout.width, layout.height, content_id, annots_str, font_id
        );
        let page_id = generator.add_object(page_dict);
        page_ids.push(page_id);
        generator.add_object("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n".to_string());
    }

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let pages_dict = format!("<< /Type /Pages\n/Kids [{}]\n/Count {}\n>>\n", kids.join(" "), page_ids.len());
    let actual_pages_id = generator.add_object(pages_dict);
    assert_eq!(actual_pages_id, pages_obj_id);
    generator.add_object(format!("<< /Type /Catalog\n/Pages {} 0 R\n>>\n", actual_pages_id));

    let pdf_data = generator.generate();
    let mut file = std::fs::File::create(output_file)?;
    std::io::Write::write_all(&mut file, &pdf_data)?;
    println!(
        "[annotate] Created {} with {} text, {} link, {} highlight annotations",
        output_file, annotations.len(), links.len(), highlights.len()
    );
    Ok(())
}

/// Create a single-page PDF with text annotations (backward compatible)
pub fn create_pdf_with_annotations(
    output_file: &str,
    text: &str,
    annotations: &[TextAnnotation],
    links: &[LinkAnnotation],
) -> Result<()> {
    let elements = crate::elements::parse_markdown(text);
    let layout = crate::pdf_generator::PageLayout::portrait();

    // Build page content
    let page_streams = build_page_streams(&elements, 12.0, true, layout, None)?;
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

        let font_dict = "<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n".to_string();
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
fn build_watermark_stream(text: &str, font_size: f32, opacity: f32, layout: &crate::pdf_generator::PageLayout) -> Vec<u8> {
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
            cos45, sin45, -sin45, cos45, cx - 100.0, cy - 50.0
        )
        .as_bytes(),
    );
    stream.extend_from_slice(format!("({}) Tj\n", escaped).as_bytes());
    stream.extend_from_slice(b"ET\n");
    stream.extend_from_slice(b"Q\n");
    stream
}

/// Form field types.
///
/// Represents the type of interactive form field that can be added to a PDF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormFieldType {
    /// Text input field
    Text,
    /// Checkbox field
    Checkbox,
    /// Radio button field
    Radio,
    /// Dropdown/combobox field
    Dropdown,
}

/// A form field to be added to a PDF.
///
/// Represents an interactive form field with its properties including
/// position, dimensions, default value, options (for radio/dropdown), and
/// whether the field is required.
///
/// # Fields
///
/// * `name` - Unique identifier for the form field
/// * `field_type` - Type of form field (Text, Checkbox, Radio, Dropdown)
/// * `x` - X position on the page (in PDF points)
/// * `y` - Y position on the page (in PDF points)
/// * `width` - Width of the field (in PDF points)
/// * `height` - Height of the field (in PDF points)
/// * `default_value` - Optional default value for the field
/// * `options` - List of options (for radio buttons and dropdowns)
/// * `required` - Whether the field must be filled
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::pdf_ops::{FormField, FormFieldType};
///
/// let field = FormField {
///     name: "firstName".to_string(),
///     field_type: FormFieldType::Text,
///     x: 100.0,
///     y: 700.0,
///     width: 200.0,
///     height: 20.0,
///     default_value: Some("John".to_string()),
///     options: vec![],
///     required: true,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    pub field_type: FormFieldType,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub default_value: Option<String>,
    pub options: Vec<String>, // For radio/dropdown
    pub required: bool,
}

/// Create a PDF with an AcroForm containing interactive form fields
pub fn create_pdf_with_form_fields(
    output_file: &str,
    text: &str,
    form_fields: &[FormField],
) -> Result<()> {
    let elements = crate::elements::parse_markdown(text);
    let layout = crate::pdf_generator::PageLayout::portrait();
    let page_streams = build_page_streams(&elements, 12.0, true, layout, None)?;
    if page_streams.is_empty() {
        return Err(anyhow!("No page content generated"));
    }

    let mut generator = crate::pdf_generator::PdfGenerator::new();
    let mut field_ids: Vec<u32> = Vec::new();

    // Create form field annotations
    for field in form_fields {
        let field_dict = create_form_field_dict(field);
        field_ids.push(generator.add_object(field_dict));
    }

    // Create AcroForm dictionary
    let kids_refs: Vec<String> = field_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let acroform_dict = format!(
        "<< /Fields [{}]\n>>\n",
        kids_refs.join(" ")
    );
    let acroform_id = generator.add_object(acroform_dict);

    let field_offset = field_ids.len() as u32;
    let pages_obj_id = field_offset + 1 + (page_streams.len() as u32) * 3 + 1;
    let mut page_ids = Vec::new();

    for (i, page_stream) in page_streams.iter().enumerate() {
        let content_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", page_stream.len()),
            page_stream.clone(),
        );
        let font_id = content_id + 2;

        // Only first page gets form fields
        let annots_str = if i == 0 && !field_ids.is_empty() {
            let refs: Vec<String> = field_ids.iter().map(|id| format!("{} 0 R", id)).collect();
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
        generator.add_object("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n".to_string());
    }

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let pages_dict = format!("<< /Type /Pages\n/Kids [{}]\n/Count {}\n>>\n", kids.join(" "), page_ids.len());
    let actual_pages_id = generator.add_object(pages_dict);

    let catalog_dict = format!(
        "<< /Type /Catalog\n/Pages {} 0 R\n/AcroForm {} 0 R\n>>\n",
        actual_pages_id, acroform_id
    );
    generator.add_object(catalog_dict);

    let pdf_data = generator.generate();
    let mut file = std::fs::File::create(output_file)?;
    std::io::Write::write_all(&mut file, &pdf_data)?;
    println!(
        "[form] Created {} with {} form fields",
        output_file,
        form_fields.len()
    );
    Ok(())
}

/// Create a form field annotation dictionary
fn create_form_field_dict(field: &FormField) -> String {
    let base_dict = format!(
        "<< /Type /Annot\n/Subtype /Widget\n\
         /Rect [{} {} {} {}]\n\
         /FT {}\n\
         /T ({})\n",
        field.x,
        field.y,
        field.x + field.width,
        field.y + field.height,
        field_type_to_pdf(&field.field_type),
        escape_pdf_meta(&field.name)
    );

    let mut dict = base_dict;

    // Add default value if present
    if let Some(ref value) = field.default_value {
        dict.push_str(&format!("/V ({})\n", escape_pdf_meta(value)));
    }

    // Add field-type specific properties
    match field.field_type {
        FormFieldType::Text => {
            dict.push_str(&format!(
                "/Ff {}\n",
                if field.required { 2 } else { 0 } // 2 = Required flag
            ));
            // Appearance for text field
            dict.push_str("/AP << /N << /Type /Appearance\n/Length 0 >> >>\n");
        }
        FormFieldType::Checkbox => {
            dict.push_str(&format!(
                "/V /Off\n/Ff {}\n",
                if field.required { 2 } else { 0 }
            ));
            // Appearance for checkbox
            dict.push_str("/AP << /N << /Type /Appearance\n/Length 0 >> >>\n");
        }
        FormFieldType::Radio => {
            if !field.options.is_empty() {
                let opts: Vec<String> = field.options.iter().map(|o| format!("({})", escape_pdf_meta(o))).collect();
                dict.push_str(&format!("/Opt [{}]\n", opts.join(" ")));
            }
            dict.push_str(&format!(
                "/V /Off\n/Ff {}\n",
                if field.required { 2 } else { 0 }
            ));
        }
        FormFieldType::Dropdown => {
            if !field.options.is_empty() {
                let opts: Vec<String> = field.options.iter().map(|o| format!("({})", escape_pdf_meta(o))).collect();
                dict.push_str(&format!("/Opt [{}]\n", opts.join(" ")));
            }
            dict.push_str(&format!(
                "/Ff {}131072\n",
                if field.required { 2 + 131072 } else { 131072 } // 131072 = Combo flag
            ));
        }
    }

    dict.push_str(">>\n");
    dict
}

/// Convert FormFieldType to PDF field type string
fn field_type_to_pdf(field_type: &FormFieldType) -> String {
    match field_type {
        FormFieldType::Text => "/Tx".to_string(),
        FormFieldType::Checkbox => "/Btn".to_string(),
        FormFieldType::Radio => "/Btn".to_string(),
        FormFieldType::Dropdown => "/Ch".to_string(),
    }
}

/// A form field detected in an existing PDF document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedFormField {
    pub name: String,
    pub field_type: String,
    pub value: Option<String>,
    pub options: Vec<String>,
    pub required: bool,
}

/// Detect all interactive form fields in an existing PDF.
///
/// Scans the PDF for widget annotations with field type entries
/// and returns their names, types, current values, and available options.
///
/// # Returns
///
/// A vector of `DetectedFormField` structs, one per field found.
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::pdf_ops::detect_form_fields;
///
/// let fields = detect_form_fields("form.pdf").unwrap();
/// for f in &fields {
///     println!("{}: {:?}", f.name, f.value);
/// }
/// ```
pub fn detect_form_fields(input_file: &str) -> Result<Vec<DetectedFormField>> {
    let pdf_bytes = fs::read(input_file)?;
    let content = String::from_utf8_lossy(&pdf_bytes);

    let mut fields = Vec::new();

    // Find all PDF objects and check if they are widget annotations
    let obj_re = regex::Regex::new(r"(?s)(\d+)\s+0\s+obj(.*?)endobj").unwrap();
    let opt_re = regex::Regex::new(r"\(([^)]*)\)").unwrap();

    for caps in obj_re.captures_iter(&content) {
        let obj_text = &caps[0];
        let obj_body = &caps[2];

        // Must be an annotation widget
        if !obj_body.contains("/Type /Annot") || !obj_body.contains("/Subtype /Widget") {
            continue;
        }

        let dict_text = obj_text;

        // Extract /T (field name)
        let name = extract_pdf_dict_value(dict_text, "/T")
            .unwrap_or_default()
            .trim_matches(|c| c == '(' || c == ')')
            .to_string();

        if name.is_empty() {
            continue;
        }

        // Extract /FT (field type)
        let field_type = extract_pdf_dict_value(dict_text, "/FT")
            .unwrap_or_default()
            .trim_start_matches('/')
            .to_string();

        // Map PDF type to readable string
        let type_str = match field_type.as_str() {
            "Tx" => "text",
            "Btn" => {
                // Distinguish checkbox/radio by presence of /Opt or /V style
                if extract_pdf_dict_value(dict_text, "/Opt").is_some() {
                    "radio"
                } else {
                    "checkbox"
                }
            }
            "Ch" => "dropdown",
            _ => "unknown",
        };

        // Extract /V (value)
        let value = extract_pdf_dict_value(dict_text, "/V").map(|v| {
            if v.starts_with('(') && v.ends_with(')') {
                v[1..v.len()-1].to_string()
            } else if v.starts_with('<') && v.ends_with('>') {
                crate::pdf::decode_pdf_hex_string(&v[1..v.len()-1])
            } else {
                v.to_string()
            }
        });

        // Extract /Opt (options list)
        let options = if let Some(opt_raw) = extract_pdf_dict_value(dict_text, "/Opt") {
            // /Opt can be [(Option1) (Option2)] or an array reference
            opt_re.captures_iter(&opt_raw)
                .map(|c| c[1].to_string())
                .collect()
        } else {
            Vec::new()
        };

        // Extract /Ff flags — bit 30 (value 2) = required
        let required = extract_pdf_dict_value(dict_text, "/Ff")
            .and_then(|f| f.parse::<u32>().ok())
            .map(|flags| (flags & 2) != 0)
            .unwrap_or(false);

        fields.push(DetectedFormField {
            name,
            field_type: type_str.to_string(),
            value,
            options,
            required,
        });
    }

    Ok(fields)
}

/// Fill existing form fields in a PDF with new values and write the result.
///
/// Reads the input PDF, finds form fields by name, updates their /V values,
/// and writes an incremental update to the output file.
///
/// # Arguments
///
/// * `input_file` - Path to the input PDF with form fields
/// * `output_file` - Path where the filled PDF will be written
/// * `field_values` - HashMap mapping field names to new values
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if filling fails.
///
/// # Example
///
/// ```rust,no_run
/// use std::collections::HashMap;
/// use pdfrs::pdf_ops::fill_form_fields;
///
/// let mut values = HashMap::new();
/// values.insert("firstName".to_string(), "Alice".to_string());
/// values.insert("age".to_string(), "30".to_string());
/// fill_form_fields("form.pdf", "filled.pdf", &values).unwrap();
/// ```
pub fn fill_form_fields(
    input_file: &str,
    output_file: &str,
    field_values: &std::collections::HashMap<String, String>,
) -> Result<()> {
    let pdf_bytes = fs::read(input_file)?;
    let content = String::from_utf8_lossy(&pdf_bytes);

    if field_values.is_empty() {
        fs::write(output_file, &pdf_bytes)?;
        return Ok(());
    }

    // Find all PDF objects and check if they are widget annotations
    let obj_re = regex::Regex::new(r"(?s)(\d+)\s+0\s+obj(.*?)endobj").unwrap();
    let v_re = regex::Regex::new(r"/V\s*\([^)]*\)").unwrap();

    let mut updated_bytes = pdf_bytes.clone();
    let mut offset_delta: isize = 0;

    for caps in obj_re.captures_iter(&content) {
        let dict_text = &caps[0];
        let obj_body = &caps[2];
        let full_match_start = caps.get(0).unwrap().start();

        // Must be a widget annotation
        if !obj_body.contains("/Type /Annot") || !obj_body.contains("/Subtype /Widget") {
            continue;
        }

        // Extract field name
        let name = extract_pdf_dict_value(dict_text, "/T")
            .unwrap_or_default()
            .trim_matches(|c| c == '(' || c == ')')
            .to_string();

        if name.is_empty() || !field_values.contains_key(&name) {
            continue;
        }

        let new_value = &field_values[&name];
        let escaped_value = escape_pdf_meta(new_value);

        let adjusted_start = ((full_match_start as isize) + offset_delta) as usize;
        let adjusted_end = adjusted_start + dict_text.len();

        if adjusted_end > updated_bytes.len() {
            continue;
        }

        let local_dict = String::from_utf8_lossy(&updated_bytes[adjusted_start..adjusted_end]);

        // Replace existing /V (...) or add /V before the closing >>
        let updated_dict = if local_dict.contains("/V ") {
            let new_v = format!("/V ({})", escaped_value);
            v_re.replace(&local_dict, &new_v).to_string()
        } else {
            // Insert /V before the final >>
            local_dict.replace(">>", &format!("/V ({})\n>>", escaped_value))
        };

        if updated_dict != *local_dict {
            let old_len = local_dict.len();
            let new_len = updated_dict.len();
            updated_bytes.splice(adjusted_start..adjusted_end, updated_dict.bytes());
            offset_delta += (new_len as isize) - (old_len as isize);
        }
    }

    fs::write(output_file, &updated_bytes)?;
    println!("[fill] Updated {} field(s) in {}", field_values.len(), output_file);
    Ok(())
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
        overlay_content.extend_from_slice(format!("{} {} {} rg\n", opacity, opacity, opacity).as_bytes());
    }
    overlay_content.extend_from_slice(b"q\n");
    overlay_content.extend_from_slice(format!("{} 0 0 {} {} {} cm\n", width, height, x, y).as_bytes());
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
        WatermarkPosition::Center => {
            (layout.width / 2.0, layout.height / 2.0, 0.0)
        }
        WatermarkPosition::TopLeft => {
            (72.0, layout.height - 72.0, 0.0)
        }
        WatermarkPosition::TopRight => {
            (layout.width - 72.0, layout.height - 72.0, 0.0)
        }
        WatermarkPosition::BottomLeft => {
            (72.0, 72.0, 0.0)
        }
        WatermarkPosition::BottomRight => {
            (layout.width - 72.0, 72.0, 0.0)
        }
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
            format!("{} {} {} {} {} {} Tm\n", cos, sin, -sin, cos, x, y).as_bytes()
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
    let (img_width, img_height) = crate::image::scale_to_fit(
        image_info.width,
        image_info.height,
        max_width,
        max_height,
    );

    let (x, y) = match position {
        WatermarkPosition::Center => {
            ((layout.width - img_width) / 2.0, (layout.height - img_height) / 2.0)
        }
        WatermarkPosition::TopLeft => {
            (36.0, layout.height - img_height - 36.0)
        }
        WatermarkPosition::TopRight => {
            (layout.width - img_width - 36.0, layout.height - img_height - 36.0)
        }
        WatermarkPosition::BottomLeft => {
            (36.0, 36.0)
        }
        WatermarkPosition::BottomRight => {
            (layout.width - img_width - 36.0, 36.0)
        }
        WatermarkPosition::Diagonal => {
            ((layout.width - img_width) / 2.0, (layout.height - img_height) / 2.0)
        }
    };

    let mut stream = Vec::new();
    stream.extend_from_slice(b"q\n");
    if opacity < 1.0 {
        stream.extend_from_slice(format!("{} {} {} rg\n", opacity, opacity, opacity).as_bytes());
    }
    stream.extend_from_slice(b"q\n");
    stream.extend_from_slice(format!("{} 0 0 {} {} {} cm\n", img_width, img_height, x, y).as_bytes());
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

/// Apply password protection and permissions to a PDF.
///
/// This function adds security settings to a PDF document, including password protection
/// and permission restrictions. Note that this is a simplified implementation that adds
/// the encryption dictionary to the PDF trailer. For production use, you would need
/// proper cryptographic libraries (like RustCrypto or openssl) for actual encryption.
///
/// # Arguments
///
/// * `input_file` - Path to the input PDF file
/// * `output_file` - Path where the protected PDF will be written
/// * `security` - Security settings including passwords and permissions
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if protection fails.
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::{pdf_ops, security};
///
/// let sec = security::PdfSecurity::new()
///     .with_user_password("secret".to_string())
///     .with_permissions(security::PdfPermissions::read_only());
///
/// pdf_ops::protect_pdf("input.pdf", "protected.pdf", &sec)
///     .expect("Failed to protect PDF");
/// ```
///
/// # Errors
///
/// This function will return an error if:
/// - The input file cannot be read
/// - The security settings are invalid
/// - Writing the output file fails
pub fn protect_pdf(input_file: &str, output_file: &str, security: &crate::security::PdfSecurity) -> Result<()> {
    security.validate()?;

    // Honest gate: do not write fake "protected" PDFs that remain plaintext.
    if security.is_protected() {
        // Fail early before touching files when encryption is requested.
        let _ = security.create_encryption_dict()?;
        return Err(anyhow!(
            "Password protection is not implemented yet; refusing to write an unprotected PDF that claims encryption"
        ));
    }

    // No passwords configured — copy through unchanged.
    let content = fs::read(input_file)?;
    fs::write(output_file, content)?;
    Ok(())
}

pub(crate) fn escape_pdf_meta(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

use sha2::{Digest, Sha256};

/// Add a digital signature to a PDF document.
///
/// This creates the PDF signature field structure and computes a SHA-256
/// content digest over the signed byte ranges. The actual PKCS#7/CMS
/// container is stored as a placeholder; external tools can replace it.
///
/// # Arguments
/// * `input_file` - Path to the original PDF
/// * `output_file` - Path for the signed PDF output
/// * `signature` - Digital signature metadata (signer, reason, location, etc.)
///
/// # Example
/// ```no_run
/// use pdfrs::{security::DigitalSignature, pdf_ops::sign_pdf};
///
/// let sig = DigitalSignature::new("Alice")
///     .with_reason("I approve this document")
///     .with_location("New York");
/// sign_pdf("input.pdf", "signed.pdf", &sig).unwrap();
/// ```
pub fn sign_pdf(input_file: &str, output_file: &str, signature: &crate::security::DigitalSignature) -> Result<()> {
    sign_pdf_with_certificate(input_file, output_file, signature, None)
}

/// Sign a PDF and optionally embed an X.509 certificate in the signature dictionary.
pub fn sign_pdf_with_certificate(
    input_file: &str,
    output_file: &str,
    signature: &crate::security::DigitalSignature,
    certificate: Option<&crate::security::SigningCertificate>,
) -> Result<()> {
    let pdf_bytes = fs::read(input_file)?;

    // Build incremental update with signature objects
    let sig = signature.clone();

    // Placeholder for signature contents (8192 hex chars = 4096 bytes)
    let contents_placeholder = "0".repeat(8192);

    // Build signature dictionary with placeholder
    let mut sig_dict = format!(
        "<< /Type /Sig\n\
         /Filter /Adobe.PPKLite\n\
         /SubFilter /adbe.pkcs7.detached\n\
         /Contents <{}>\n\
         /ByteRange [0 0 0 0]\n",
        contents_placeholder
    );
    if let Some(ref date) = sig.date {
        sig_dict.push_str(&format!(" /M (D:{})\n", escape_pdf_meta(date)));
    }
    sig_dict.push_str(&format!(" /Name ({})\n", escape_pdf_meta(&sig.signer_name)));
    if let Some(ref reason) = sig.reason {
        sig_dict.push_str(&format!(" /Reason ({})\n", escape_pdf_meta(reason)));
    }
    if let Some(ref location) = sig.location {
        sig_dict.push_str(&format!(" /Location ({})\n", escape_pdf_meta(location)));
    }
    if let Some(ref contact) = sig.contact_info {
        sig_dict.push_str(&format!(" /ContactInfo ({})\n", escape_pdf_meta(contact)));
    }
    if let Some(cert) = certificate {
        let der_hex = crate::security::certificate_pem_to_der_hex(&cert.pem)?;
        sig_dict.push_str(&format!(" /Cert <{}>\n", der_hex));
    }
    sig_dict.push_str(">>");

    // Rebuild with proper PDF objects
    let original_len = pdf_bytes.len();
    let mut output = pdf_bytes.clone();

    // Find the last %%EOF
    let last_eof = output.windows(5).rposition(|w| w == b"%%EOF").unwrap_or(0);
    let startxref_pos = output[..last_eof].windows(9).rposition(|w| w == b"startxref").unwrap_or(0);
    let xref_offset: usize = String::from_utf8_lossy(&output[startxref_pos + 9..last_eof])
        .trim()
        .parse()
        .unwrap_or(0);

    // Find catalog reference in trailer
    let trailer_end = output[startxref_pos..].iter().position(|&b| b == b'>').unwrap_or(0);
    let trailer_text = String::from_utf8_lossy(&output[startxref_pos..startxref_pos + trailer_end]);
    let catalog_ref = trailer_text
        .lines()
        .find(|l| l.contains("/Root"))
        .and_then(|l| {
            l.split("/Root")
                .nth(1)?
                .split_whitespace()
                .next()
                .map(|s| s.trim())
        })
        .unwrap_or("");

    // Build incremental update
    let update_start = original_len;
    let mut update = Vec::new();

    // Signature dictionary object
    let sig_obj_num = 999; // Use high number to avoid conflicts
    let sig_dict_obj = format!("{} 0 obj\n{}\nendobj\n", sig_obj_num, sig_dict);
    update.extend_from_slice(sig_dict_obj.as_bytes());

    // Signature field (widget annotation + form field)
    let field_obj_num = sig_obj_num + 1;
    let field_dict = format!(
        "{} 0 obj\n<< /Type /Annot\n\
         /Subtype /Widget\n\
         /FT /Sig\n\
         /T (Signature1)\n\
         /V {} 0 R\n\
         /P 1 0 R\n\
         /Rect [0 0 0 0]\n\
         /F 132\n\
         >>\nendobj\n",
        field_obj_num, sig_obj_num
    );
    update.extend_from_slice(field_dict.as_bytes());

    // New catalog with /AcroForm
    let new_catalog_num = sig_obj_num + 2;
    let new_catalog = format!(
        "{} 0 obj\n<< /Type /Catalog\n\
         /Pages {}\n\
         /AcroForm << /Fields [{} 0 R] /SigFlags 3 >>\n\
         >>\nendobj\n",
        new_catalog_num,
        if catalog_ref.is_empty() { "1 0 R".to_string() } else { catalog_ref.to_string() },
        field_obj_num
    );
    update.extend_from_slice(new_catalog.as_bytes());

    // New trailer pointing to new catalog
    let xref_offset_new = update_start;
    let xref = format!(
        "xref\n\
         0 1\n\
         0000000000 65535 f \n\
         {} 3\n\
         {:010} 00000 n \n\
         {:010} 00000 n \n\
         {:010} 00000 n \n",
        sig_obj_num,
        xref_offset_new,
        xref_offset_new + sig_dict_obj.len(),
        xref_offset_new + sig_dict_obj.len() + field_dict.len()
    );
    update.extend_from_slice(xref.as_bytes());

    let trailer = format!(
        "trailer\n<< /Size {} /Root {} 0 R /Prev {} >>\nstartxref\n{}\n%%EOF\n",
        new_catalog_num + 1,
        new_catalog_num,
        xref_offset,
        update_start
    );
    update.extend_from_slice(trailer.as_bytes());

    // Append update to output
    output.extend_from_slice(&update);

    // Now compute byte range and content hash
    let full_output = output.clone();
    let contents_marker = format!("Contents <{}", contents_placeholder);
    let contents_start = full_output
        .windows(contents_marker.len())
        .position(|w| w == contents_marker.as_bytes())
        .ok_or_else(|| anyhow!("Could not find signature contents placeholder"))?;

    // ByteRange: [0, contents_start_of_value, contents_end_of_value, remaining]
    let value_start = contents_start + 1; // Point to '<' in "Contents <"
    let value_end = contents_start + contents_marker.len() + 1; // After '>'

    let byte_range = [0u32,
        value_start as u32,
        value_end as u32,
        (full_output.len() - value_end) as u32];

    // Compute SHA-256 over the byte ranges
    let mut hasher = Sha256::new();
    hasher.update(&full_output[0..value_start]);
    hasher.update(&full_output[value_end..]);
    let hash = hasher.finalize();
    let hash_hex = hash.iter().map(|b| format!("{:02x}", b)).collect::<String>();

    // Replace placeholder with hash (pad with zeros to maintain length)
    let padded_hash = format!("{:0<width$}", hash_hex, width = contents_placeholder.len());
    let old_marker = format!("Contents <{}", contents_placeholder);
    let new_marker = format!("Contents <{}", padded_hash);
    let output_str = String::from_utf8_lossy(&full_output);
    let final_output = output_str.replace(&old_marker, &new_marker);

    // Replace ByteRange placeholder
    let final_output = final_output.replace(
        "/ByteRange [0 0 0 0]",
        &format!(
            "/ByteRange [{} {} {} {}]",
            byte_range[0], byte_range[1], byte_range[2], byte_range[3]
        ),
    );

    fs::write(output_file, final_output)?;

    println!(
        "[sign] Signed {} -> {} (signer: {}, hash: {})",
        input_file, output_file, sig.signer_name, &hash_hex[..16]
    );

    Ok(())
}

/// Verify that a PDF contains a digital signature structure.
///
/// This checks for the presence of signature fields and reports
/// basic signature metadata. It does NOT cryptographically verify
/// the signature against a certificate chain.
///
/// Returns a list of signature info found in the document.
pub fn verify_pdf_signature(input_file: &str) -> Result<Vec<SignatureInfo>> {
    let pdf_bytes = fs::read(input_file)?;
    let text = String::from_utf8_lossy(&pdf_bytes);
    let mut results = Vec::new();

    // Find all "N 0 obj" blocks and check for signature dictionaries
    // Use [\s\S] instead of . to match newlines inside dictionary content
    let obj_re = regex::Regex::new(r"(?s)(\d+)\s+0\s+obj\s+<<(.+?)>>\s+endobj").unwrap();
    for caps in obj_re.captures_iter(&text) {
        let dict_content = &caps[2];
        if dict_content.contains("/Type /Sig") || dict_content.contains("/Type/Sig") {
            let name = extract_pdf_dict_value(dict_content, "/Name").unwrap_or_default();
            let reason = extract_pdf_dict_value(dict_content, "/Reason");
            let location = extract_pdf_dict_value(dict_content, "/Location");
            let date = extract_pdf_dict_value(dict_content, "/M");
            let byte_range = extract_pdf_dict_value(dict_content, "/ByteRange");
            let cert_hex = extract_pdf_dict_value(dict_content, "/Cert");
            let (certificate_subject, certificate_fingerprint) = cert_hex
                .as_ref()
                .and_then(|hex| parse_cert_hex_metadata(hex))
                .map(|(subject, fp)| (Some(subject), Some(fp)))
                .unwrap_or((None, None));

            results.push(SignatureInfo {
                signer_name: name,
                reason,
                location,
                date,
                byte_range,
                certificate_subject,
                certificate_fingerprint,
                valid: false,
            });
        }
    }

    Ok(results)
}

/// Extract embedded X.509 certificates from PDF signature dictionaries.
pub fn extract_certificates_from_pdf_bytes(data: &[u8]) -> Result<Vec<crate::security::SigningCertificate>> {
    let text = String::from_utf8_lossy(data);
    let obj_re = regex::Regex::new(r"(?s)(\d+)\s+0\s+obj\s+<<(.+?)>>\s+endobj").unwrap();
    let mut certs = Vec::new();
    let mut index = 0usize;

    for caps in obj_re.captures_iter(&text) {
        let dict_content = &caps[2];
        if dict_content.contains("/Type /Sig") || dict_content.contains("/Type/Sig") {
            if let Some(hex) = extract_pdf_dict_value(dict_content, "/Cert") {
                if let Ok(cert) = der_hex_to_certificate(&hex, index) {
                    certs.push(cert);
                    index += 1;
                }
            }
        }
    }

    Ok(certs)
}

/// Extract embedded certificates from a PDF file.
pub fn extract_certificates_from_pdf(input_file: &str) -> Result<Vec<crate::security::SigningCertificate>> {
    let data = fs::read(input_file)?;
    extract_certificates_from_pdf_bytes(&data)
}

fn der_hex_to_certificate(hex: &str, index: usize) -> Result<crate::security::SigningCertificate> {
    let cleaned: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if !cleaned.len().is_multiple_of(2) {
        return Err(anyhow!("Invalid certificate hex length"));
    }
    let der: Vec<u8> = cleaned
        .as_bytes()
        .chunks(2)
        .map(|chunk| u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16))
        .collect::<Result<Vec<_>, _>>()?;
    let b64 = encode_base64(&der);
    let pem = format!("-----BEGIN CERTIFICATE-----\n{b64}\n-----END CERTIFICATE-----\n");
    crate::security::parse_certificate_pem(format!("cert-{index}"), &pem)
}

fn encode_base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let triple = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(TABLE[((triple >> 18) & 63) as usize] as char);
        out.push(TABLE[((triple >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn parse_cert_hex_metadata(hex: &str) -> Option<(String, String)> {
    let cert = der_hex_to_certificate(hex, 0).ok()?;
    Some((cert.subject, cert.fingerprint_sha256))
}

fn extract_pdf_dict_value(dict: &str, key: &str) -> Option<String> {
    // Search for key as a standalone token (followed by whitespace or end)
    let pos = dict.match_indices(key)
        .find(|(i, _)| {
            let end = i + key.len();
            end == dict.len() || dict[end..].starts_with(|c: char| c.is_whitespace() || c == '(' || c == '<' || c == '[')
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
        let end = name_after.find(|c: char| c.is_whitespace() || c == '/' || c == '>' || c == '[').unwrap_or(name_after.len());
        Some(name_after[..end].to_string())
    } else {
        let end = after.find(|c: char| c.is_whitespace() || c == '/' || c == '>').unwrap_or(after.len());
        Some(after[..end].to_string())
    }
}

/// A single text fragment with its position in a PDF content stream
#[derive(Debug, Clone, PartialEq)]
struct TextFragment {
    text: String,
    x: f32,
    y: f32,
}

/// Extract tables from a PDF and return them as CSV strings.
///
/// This function analyzes the text positioning in PDF content streams
/// to heuristically detect tables. It groups text fragments by Y position
/// into rows, then sorts by X position within each row to form columns.
///
/// # Returns
/// A vector of CSV strings, one per detected table.
pub fn extract_tables_from_pdf(input_file: &str) -> Result<Vec<String>> {
    use crate::pdf::{PdfDocument, PdfObject};

    let doc = PdfDocument::load_from_file(input_file)?;
    let mut all_fragments: Vec<TextFragment> = Vec::new();

    // Regex patterns for text extraction with positioning
    let tj_re = regex::Regex::new(r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)\s*Tj").unwrap();
    let tj_hex_re = regex::Regex::new(r"<([0-9a-fA-F\s]+)>\s*Tj").unwrap();
    let td_re = regex::Regex::new(r"([\d.\-]+)\s+([\d.\-]+)\s+T[dD]").unwrap();
    let tm_re = regex::Regex::new(r"[\d.\-]+\s+[\d.\-]+\s+[\d.\-]+\s+[\d.\-]+\s+([\d.\-]+)\s+([\d.\-]+)\s+Tm").unwrap();

    for obj in doc.objects.values() {
        if let PdfObject::Stream { data, .. } = obj {
            let processed_data = crate::compression::decompress_deflate(data).unwrap_or_else(|_| data.to_vec());
            let content = String::from_utf8_lossy(&processed_data);

            let mut current_x: f32 = 0.0;
            let mut current_y: f32 = 0.0;

            for line in content.lines() {
                let line = line.trim();

                // Track positioning
                if let Some(caps) = td_re.captures(line)
                    && let (Ok(x), Ok(y)) = (caps[1].parse::<f32>(), caps[2].parse::<f32>()) {
                        current_x = x;
                        current_y = y;
                    }
                if let Some(caps) = tm_re.captures(line)
                    && let (Ok(x), Ok(y)) = (caps[1].parse::<f32>(), caps[2].parse::<f32>()) {
                        current_x = x;
                        current_y = y;
                    }

                // Extract text fragments with current position
                for caps in tj_re.captures_iter(line) {
                    let extracted = &caps[1];
                    let unescaped = crate::pdf::unescape_pdf_string(extracted);
                    if !unescaped.trim().is_empty() {
                        all_fragments.push(TextFragment {
                            text: unescaped.trim().to_string(),
                            x: current_x,
                            y: current_y,
                        });
                    }
                }

                for caps in tj_hex_re.captures_iter(line) {
                    let hex_str = caps[1].replace(char::is_whitespace, "");
                    let decoded = crate::pdf::decode_pdf_hex_string(&hex_str);
                    if !decoded.trim().is_empty() {
                        all_fragments.push(TextFragment {
                            text: decoded.trim().to_string(),
                            x: current_x,
                            y: current_y,
                        });
                    }
                }
            }
        }
    }

    if all_fragments.is_empty() {
        return Ok(Vec::new());
    }

    // Sort by Y descending (PDF coordinates: 0,0 is bottom-left)
    all_fragments.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap());

    // Group into rows by Y position (within tolerance)
    let y_tolerance = 3.0; // points
    let mut rows: Vec<Vec<TextFragment>> = Vec::new();
    let mut current_row: Vec<TextFragment> = Vec::new();
    let mut current_y = all_fragments[0].y;

    for frag in all_fragments {
        let frag_y = frag.y;
        if (frag_y - current_y).abs() <= y_tolerance {
            current_row.push(frag);
        } else {
            if !current_row.is_empty() {
                current_row.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
                rows.push(current_row);
            }
            current_row = vec![frag];
            current_y = frag_y;
        }
    }
    if !current_row.is_empty() {
        current_row.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        rows.push(current_row);
    }

    // Merge rows with very similar Y positions (same line, slight variations)
    let mut merged_rows: Vec<Vec<TextFragment>> = Vec::new();
    for row in rows {
        if let Some(last) = merged_rows.last_mut() {
            let last_y = last.iter().map(|f| f.y).sum::<f32>() / last.len() as f32;
            let row_y = row.iter().map(|f| f.y).sum::<f32>() / row.len() as f32;
            if (last_y - row_y).abs() <= y_tolerance {
                last.extend(row);
                last.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
                continue;
            }
        }
        merged_rows.push(row);
    }

    // Detect tables: find consecutive rows with similar structure
    let mut tables: Vec<Vec<Vec<String>>> = Vec::new();
    let mut current_table: Vec<Vec<String>> = Vec::new();
    let x_tolerance = 8.0; // points for column grouping

    for row in &merged_rows {
        let cells = group_row_into_cells(row, x_tolerance);
        if cells.len() >= 2 {
            current_table.push(cells);
        } else if !current_table.is_empty() {
            if current_table.len() >= 2 {
                tables.push(current_table);
            }
            current_table = Vec::new();
        }
    }
    if !current_table.is_empty() && current_table.len() >= 2 {
        tables.push(current_table);
    }

    // Convert tables to CSV
    let mut csv_outputs = Vec::new();
    for table in tables {
        let mut csv = String::new();
        for row in table {
            let escaped: Vec<String> = row.iter().map(|cell| escape_csv_field(cell)).collect();
            csv.push_str(&escaped.join(","));
            csv.push('\n');
        }
        csv_outputs.push(csv);
    }

    Ok(csv_outputs)
}

fn group_row_into_cells(row: &[TextFragment], x_tolerance: f32) -> Vec<String> {
    if row.is_empty() {
        return Vec::new();
    }

    let mut cells: Vec<Vec<String>> = Vec::new();
    let mut current_cell: Vec<String> = Vec::new();
    let mut last_x = row[0].x;

    for frag in row {
        if (frag.x - last_x).abs() > x_tolerance && !current_cell.is_empty() {
            cells.push(current_cell);
            current_cell = Vec::new();
        }
        current_cell.push(frag.text.clone());
        last_x = frag.x;
    }
    if !current_cell.is_empty() {
        cells.push(current_cell);
    }

    cells.into_iter().map(|parts| parts.join(" ")).collect()
}

fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        let escaped = field.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    } else {
        field.to_string()
    }
}

/// A text fragment augmented with font information for structure detection.
#[derive(Debug, Clone, PartialEq)]
struct StyledTextFragment {
    text: String,
    x: f32,
    y: f32,
    font_size: f32,
    font_name: String,
}

/// A detected heading with its level and position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedHeading {
    pub level: u8,
    pub text: String,
    pub page_hint: Option<u32>,
}

/// A detected section of the document (content between headings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedSection {
    pub title: Option<String>,
    pub level: u8,
    pub content_lines: Vec<String>,
    pub has_table: bool,
}

/// The overall structure of a PDF document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentStructure {
    pub headings: Vec<DetectedHeading>,
    pub sections: Vec<DetectedSection>,
    pub estimated_page_count: u32,
    pub body_font_size: f32,
}

/// Detect the document structure (headings, sections, tables) of an existing PDF.
///
/// Analyzes text positioning and font sizes in PDF content streams to heuristically
/// identify headings, body text sections, and tables. Headings are detected when
/// a line's font size is significantly larger than the dominant (body) font size.
///
/// # Returns
///
/// A `DocumentStructure` containing headings, sections, and metadata.
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::pdf_ops::detect_document_structure;
///
/// let structure = detect_document_structure("report.pdf").unwrap();
/// for h in &structure.headings {
///     println!("H{}: {}", h.level, h.text);
/// }
/// ```
pub fn detect_document_structure(input_file: &str) -> Result<DocumentStructure> {
    use crate::pdf::{PdfDocument, PdfObject};

    let doc = PdfDocument::load_from_file(input_file)?;
    let mut all_fragments: Vec<StyledTextFragment> = Vec::new();

    let tj_re = regex::Regex::new(r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)\s*Tj").unwrap();
    let tj_hex_re = regex::Regex::new(r"<([0-9a-fA-F\s]+)>\s*Tj").unwrap();
    let td_re = regex::Regex::new(r"([\d.\-]+)\s+([\d.\-]+)\s+T[dD]").unwrap();
    let tm_re = regex::Regex::new(r"([\d.\-]+)\s+([\d.\-]+)\s+([\d.\-]+)\s+([\d.\-]+)\s+([\d.\-]+)\s+([\d.\-]+)\s+Tm").unwrap();
    let tf_re = regex::Regex::new(r"/(\S+)\s+([\d.\-]+)\s+Tf").unwrap();

    for obj in doc.objects.values() {
        if let PdfObject::Stream { data, .. } = obj {
            let processed_data = crate::compression::decompress_deflate(data).unwrap_or_else(|_| data.to_vec());
            let content = String::from_utf8_lossy(&processed_data);

            let mut current_x: f32 = 0.0;
            let mut current_y: f32 = 0.0;
            let mut current_font_size: f32 = 12.0;
            let mut current_font_name: String = String::new();
            let mut tm_scale: f32 = 1.0;

            for line in content.lines() {
                let line = line.trim();

                // Track font change: /FontName size Tf
                if let Some(caps) = tf_re.captures(line)
                    && let Ok(size) = caps[2].parse::<f32>() {
                        current_font_name = caps[1].to_string();
                        current_font_size = size;
                    }

                // Track positioning
                if let Some(caps) = td_re.captures(line)
                    && let (Ok(x), Ok(y)) = (caps[1].parse::<f32>(), caps[2].parse::<f32>()) {
                        current_x = x;
                        current_y = y;
                    }
                if let Some(caps) = tm_re.captures(line)
                    && let (Ok(a), Ok(_d), Ok(x), Ok(y)) = (caps[1].parse::<f32>(), caps[4].parse::<f32>(), caps[5].parse::<f32>(), caps[6].parse::<f32>()) {
                        current_x = x;
                        current_y = y;
                        // Effective font scale from matrix (a = x-scale, d = y-scale)
                        tm_scale = a.abs();
                        // Also adjust font size by y-scale if it's meaningful
                        if let Ok(d) = caps[4].parse::<f32>()
                            && d.abs() > 0.01 {
                                tm_scale = d.abs();
                            }
                    }

                // Extract text fragments
                for caps in tj_re.captures_iter(line) {
                    let extracted = &caps[1];
                    let unescaped = crate::pdf::unescape_pdf_string(extracted);
                    if !unescaped.trim().is_empty() {
                        all_fragments.push(StyledTextFragment {
                            text: unescaped.trim().to_string(),
                            x: current_x,
                            y: current_y,
                            font_size: current_font_size * tm_scale,
                            font_name: current_font_name.clone(),
                        });
                    }
                }

                for caps in tj_hex_re.captures_iter(line) {
                    let hex_str = caps[1].replace(char::is_whitespace, "");
                    let decoded = crate::pdf::decode_pdf_hex_string(&hex_str);
                    if !decoded.trim().is_empty() {
                        all_fragments.push(StyledTextFragment {
                            text: decoded.trim().to_string(),
                            x: current_x,
                            y: current_y,
                            font_size: current_font_size * tm_scale,
                            font_name: current_font_name.clone(),
                        });
                    }
                }
            }
        }
    }

    if all_fragments.is_empty() {
        return Ok(DocumentStructure {
            headings: Vec::new(),
            sections: Vec::new(),
            estimated_page_count: 1,
            body_font_size: 12.0,
        });
    }

    // Sort by Y descending (PDF: 0,0 bottom-left)
    all_fragments.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap());

    // Group into lines by Y position
    let y_tolerance = 3.0;
    let mut lines: Vec<Vec<StyledTextFragment>> = Vec::new();
    let mut current_line: Vec<StyledTextFragment> = Vec::new();
    let mut current_y = all_fragments[0].y;

    for frag in &all_fragments {
        let frag_y = frag.y;
        if (frag_y - current_y).abs() <= y_tolerance {
            current_line.push(frag.clone());
        } else {
            if !current_line.is_empty() {
                current_line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
                lines.push(current_line);
            }
            current_line = vec![frag.clone()];
            current_y = frag_y;
        }
    }
    if !current_line.is_empty() {
        current_line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        lines.push(current_line);
    }

    // Merge very close lines (same visual line)
    let mut merged_lines: Vec<Vec<StyledTextFragment>> = Vec::new();
    for line in lines {
        if let Some(last) = merged_lines.last_mut() {
            let last_y = last.iter().map(|f| f.y).sum::<f32>() / last.len() as f32;
            let this_y = line.iter().map(|f| f.y).sum::<f32>() / line.len() as f32;
            if (this_y - last_y).abs() <= 1.5 {
                last.extend(line);
                last.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
                continue;
            }
        }
        merged_lines.push(line);
    }

    // Compute body font size (most common non-zero size)
    let mut size_counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for line in &merged_lines {
        for frag in line {
            let size_key = (frag.font_size.round() as u32).max(1);
            *size_counts.entry(size_key).or_insert(0) += 1;
        }
    }
    let body_font_size = size_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(size, _)| size as f32)
        .unwrap_or(12.0);

    // Identify headings: font size >= 1.5x body, or bold font name, or short line with large font
    let mut headings: Vec<DetectedHeading> = Vec::new();
    let mut sections: Vec<DetectedSection> = Vec::new();
    let mut current_section_lines: Vec<String> = Vec::new();
    let mut current_section_level: u8 = 0;
    let mut current_section_title: Option<String> = None;

    for line in &merged_lines {
        let line_text: String = line.iter().map(|f| &f.text as &str).collect::<Vec<_>>().join(" ");
        if line_text.trim().is_empty() {
            continue;
        }

        let avg_font_size = line.iter().map(|f| f.font_size).sum::<f32>() / line.len().max(1) as f32;
        let is_bold = line.iter().any(|f| {
            let name = f.font_name.to_lowercase();
            name.contains("bold") || name.contains("heavy") || name.contains("black")
        });
        let word_count = line_text.split_whitespace().count();

        // Heading heuristic
        let is_heading = if avg_font_size >= body_font_size * 2.0 {
            // Very large font → H1
            true
        } else if avg_font_size >= body_font_size * 1.5 {
            // Large font → H2
            true
        } else if is_bold && word_count <= 10 && avg_font_size >= body_font_size * 1.1 {
            // Bold and short → could be heading
            true
        } else {
            false
        };

        if is_heading {
            // Save previous section
            if !current_section_lines.is_empty() || current_section_title.is_some() {
                sections.push(DetectedSection {
                    title: current_section_title.clone(),
                    level: current_section_level,
                    content_lines: current_section_lines.clone(),
                    has_table: false, // detected later
                });
            }

            let level = if avg_font_size >= body_font_size * 2.0 {
                1
            } else if avg_font_size >= body_font_size * 1.5 {
                2
            } else {
                3
            };

            headings.push(DetectedHeading {
                level,
                text: line_text.trim().to_string(),
                page_hint: None,
            });

            current_section_title = Some(line_text.trim().to_string());
            current_section_level = level;
            current_section_lines = Vec::new();
        } else {
            current_section_lines.push(line_text.trim().to_string());
        }
    }

    // Push final section
    if !current_section_lines.is_empty() || current_section_title.is_some() {
        sections.push(DetectedSection {
            title: current_section_title,
            level: current_section_level,
            content_lines: current_section_lines,
            has_table: false,
        });
    }

    // Estimate page count from Y range (A4 = 842 pts height)
    let y_min = all_fragments.iter().map(|f| f.y).fold(f32::INFINITY, f32::min);
    let y_max = all_fragments.iter().map(|f| f.y).fold(f32::NEG_INFINITY, f32::max);
    let estimated_pages = ((y_max - y_min) / 800.0).ceil().max(1.0) as u32;

    Ok(DocumentStructure {
        headings,
        sections,
        estimated_page_count: estimated_pages,
        body_font_size,
    })
}

/// Information about a detected digital signature in a PDF
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureInfo {
    /// Name of the signer
    pub signer_name: String,
    /// Reason for signing
    pub reason: Option<String>,
    /// Signing location
    pub location: Option<String>,
    /// Signing date
    pub date: Option<String>,
    /// Byte range string
    pub byte_range: Option<String>,
    /// Certificate subject from embedded `/Cert` entry
    pub certificate_subject: Option<String>,
    /// SHA-256 fingerprint of embedded certificate DER
    pub certificate_fingerprint: Option<String>,
    /// Whether the signature is cryptographically valid (always false in this simplified check)
    pub valid: bool,
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
        let is_image = dictionary.get("Subtype")
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
        let filter = dictionary.get("Filter")
            .and_then(|v| match v {
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

/// Create a PDF portfolio (collection) that bundles multiple files.
///
/// A portfolio PDF wraps embedded files in a special `/Collection` catalog entry
/// that PDF readers can display as a navigable file grid or list.
///
/// # Arguments
///
/// * `output_file` — Path for the portfolio PDF output
/// * `files` — List of `(path, description)` tuples to embed
/// * `title` — Optional portfolio title
///
/// # Example
/// ```no_run
/// use pdfrs::pdf_ops::create_portfolio_pdf;
///
/// let files = vec![
///     ("report.pdf".to_string(), "Annual report".to_string()),
///     ("data.csv".to_string(), "Raw data".to_string()),
/// ];
/// create_portfolio_pdf("portfolio.pdf", &files, Some("Q3 Documents")).unwrap();
/// ```
pub fn create_portfolio_pdf(
    output_file: &str,
    files: &[(String, String)],
    title: Option<&str>,
) -> Result<()> {
    use crate::pdf::{PdfDocument, PdfObject, PdfValue};
    use std::collections::HashMap;

    let mut doc = PdfDocument::new();

    // Create a catalog object so embed_file can update it
    let catalog_dict = HashMap::new();
    let catalog_id = 1;
    doc.objects.insert(catalog_id, PdfObject::Dictionary(catalog_dict));
    doc.catalog = catalog_id;

    // Embed each file
    let mut file_specs: Vec<(String, u32)> = Vec::new(); // (filename, file_spec_object_id)
    for (path, _desc) in files {
        let data = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("Cannot read {}: {}", path, e))?;
        let filename = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);
        let fs_id = doc.embed_file(filename, &data)?;
        file_specs.push((filename.to_string(), fs_id));
    }

    // Build the /Collection dictionary with a simple schema
    let mut collection_dict = HashMap::new();
    collection_dict.insert("Type".to_string(), PdfValue::Object(PdfObject::String("/Collection".to_string())));
    collection_dict.insert("View".to_string(), PdfValue::Object(PdfObject::String("/D".to_string()))); // Detailed list view

    // Schema — two columns: Filename and Description
    let mut schema_entries = Vec::new();
    schema_entries.push("/Name << /Type /F /O << /D [ (Name) ] >> >>".to_string());
    schema_entries.push("/Description << /Type /Desc /O << /D [ (Description) ] >> >>".to_string());
    let schema = format!("<< {} >>", schema_entries.join(" "));
    collection_dict.insert("Schema".to_string(), PdfValue::Object(PdfObject::String(schema)));

    // Sort entries for the portfolio
    let sort = "<< /S /Name /A true >>".to_string();
    collection_dict.insert("Sort".to_string(), PdfValue::Object(PdfObject::String(sort)));

    // Add collection object
    let next_id = doc.objects.keys().copied().max().unwrap_or(0) + 1;
    let collection_id = next_id;
    doc.objects.insert(collection_id, PdfObject::Dictionary(collection_dict));

    // Wire /Collection into catalog
    if let Some(PdfObject::Dictionary(catalog_dict)) = doc.objects.get_mut(&doc.catalog) {
        catalog_dict.insert("Collection".to_string(), PdfValue::Object(PdfObject::String(format!("{} 0 R", collection_id))));
        // Title if provided
        if let Some(t) = title {
            catalog_dict.insert("Title".to_string(), PdfValue::Object(PdfObject::String(format!("({})", escape_pdf_meta(t)))));
        }
    }

    std::fs::write(output_file, doc.to_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_pdf_with_3d_annotation_bytes() {
        let u3d = b"U3D\0fake-model-bytes-for-structure-test";
        let annot = ThreeDAnnotation {
            contents: "Demo".into(),
            activate_on_open: true,
            ..Default::default()
        };
        let bytes = create_pdf_with_3d_annotation_bytes("3D Demo", u3d, &annot).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(pdf_contains_3d_u3d(&bytes));
        assert!(bytes.windows(u3d.len()).any(|w| w == u3d));
        let validation = crate::pdf::validate_pdf_bytes(&bytes);
        assert!(validation.valid, "{:?}", validation.errors);
    }

    #[test]
    fn test_3d_annotation_rejects_empty_u3d() {
        let annot = ThreeDAnnotation::default();
        let err = create_pdf_with_3d_annotation_bytes("x", b"", &annot).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn test_pdf_metadata_info_dict() {
        let meta = PdfMetadata {
            title: Some("Test Title".into()),
            author: Some("Test Author".into()),
            subject: None,
            keywords: None,
            creator: None,
            custom_fields: std::collections::HashMap::new(),
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
    fn test_highlight_annotation_struct() {
        let hl = HighlightAnnotation {
            x: 72.0,
            y: 700.0,
            width: 200.0,
            height: 12.0,
            color_r: 1.0,
            color_g: 1.0,
            color_b: 0.0,
        };
        assert_eq!(hl.color_r, 1.0);
        assert_eq!(hl.color_g, 1.0);
        assert_eq!(hl.color_b, 0.0);
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
    fn test_custom_metadata_fields() {
        let mut metadata = PdfMetadata::new();
        metadata.add_custom_field("CustomField1".to_string(), "Value1".to_string());
        metadata.add_custom_field("CustomField2".to_string(), "Value2".to_string());

        assert_eq!(metadata.get_custom_field("CustomField1"), Some(&"Value1".to_string()));
        assert_eq!(metadata.get_custom_field("CustomField2"), Some(&"Value2".to_string()));
        assert_eq!(metadata.get_custom_field("NonExistent"), None);

        let removed = metadata.remove_custom_field("CustomField1");
        assert_eq!(removed, Some("Value1".to_string()));
        assert_eq!(metadata.get_custom_field("CustomField1"), None);

        let dict = metadata.to_info_dict();
        assert!(dict.contains("/CustomField2 (Value2)"));
    }

    #[test]
    fn test_metadata_info_dict_with_custom_fields() {
        let mut metadata = PdfMetadata {
            title: Some("Test Title".to_string()),
            author: Some("Test Author".to_string()),
            creator: Some("Test Creator".to_string()),
            ..Default::default()
        };
        metadata.add_custom_field("Version".to_string(), "1.0".to_string());
        metadata.add_custom_field("Company".to_string(), "ACME Corp".to_string());

        let dict = metadata.to_info_dict();
        assert!(dict.contains("/Title (Test Title)"));
        assert!(dict.contains("/Author (Test Author)"));
        assert!(dict.contains("/Creator (Test Creator)"));
        assert!(dict.contains("/Version (1.0)"));
        assert!(dict.contains("/Company (ACME Corp)"));
        assert!(dict.contains("/Producer (pdf-cli)"));
    }

    #[test]
    fn test_merge_metadata() {
        let mut base = PdfMetadata {
            title: Some("Base Title".to_string()),
            author: Some("Base Author".to_string()),
            ..Default::default()
        };
        base.add_custom_field("BaseField".to_string(), "BaseValue".to_string());

        let mut new_meta = PdfMetadata {
            title: Some("New Title".to_string()),
            subject: Some("New Subject".to_string()),
            ..Default::default()
        };
        new_meta.add_custom_field("NewField".to_string(), "NewValue".to_string());

        let merged = merge_metadata(&base, &new_meta);

        assert_eq!(merged.title, Some("New Title".to_string())); // Overwritten
        assert_eq!(merged.author, Some("Base Author".to_string())); // Preserved
        assert_eq!(merged.subject, Some("New Subject".to_string())); // Added
        assert_eq!(merged.get_custom_field("BaseField"), Some(&"BaseValue".to_string())); // Preserved
        assert_eq!(merged.get_custom_field("NewField"), Some(&"NewValue".to_string())); // Added
    }

    #[test]
    fn test_unescape_pdf_string() {
        assert_eq!(unescape_pdf_string("hello"), "hello");
        assert_eq!(unescape_pdf_string(r"hello\(world\)"), "hello(world)");
        assert_eq!(unescape_pdf_string(r"line1\nline2"), "line1\nline2");
        assert_eq!(unescape_pdf_string(r"tab\there"), "tab\there");
        assert_eq!(unescape_pdf_string(r"\050"), "("); // Octal for '('
        assert_eq!(unescape_pdf_string(r"\051"), ")"); // Octal for ')'
    }

    #[test]
    fn test_extract_pdf_string_field() {
        let content = r"<< /Title (Test Title) /Author (Test \(Author\) ) /Subject None >>";
        assert_eq!(extract_pdf_string_field(content, "/Title"), Some("Test Title".to_string()));
        assert_eq!(extract_pdf_string_field(content, "/Author"), Some("Test (Author) ".to_string()));
        assert_eq!(extract_pdf_string_field(content, "/Subject"), None);
        assert_eq!(extract_pdf_string_field(content, "/NonExistent"), None);
    }

    #[test]
    fn test_form_field_struct() {
        let field = FormField {
            name: "firstName".to_string(),
            field_type: FormFieldType::Text,
            x: 100.0,
            y: 700.0,
            width: 200.0,
            height: 20.0,
            default_value: Some("John".to_string()),
            options: vec![],
            required: true,
        };
        assert_eq!(field.name, "firstName");
        assert_eq!(field.field_type, FormFieldType::Text);
        assert!(field.required);
        assert_eq!(field.default_value, Some("John".to_string()));
    }

    #[test]
    fn test_field_type_to_pdf() {
        assert_eq!(field_type_to_pdf(&FormFieldType::Text), "/Tx");
        assert_eq!(field_type_to_pdf(&FormFieldType::Checkbox), "/Btn");
        assert_eq!(field_type_to_pdf(&FormFieldType::Radio), "/Btn");
        assert_eq!(field_type_to_pdf(&FormFieldType::Dropdown), "/Ch");
    }

    #[test]
    fn test_create_form_field_dict_text() {
        let field = FormField {
            name: "username".to_string(),
            field_type: FormFieldType::Text,
            x: 50.0,
            y: 600.0,
            width: 150.0,
            height: 18.0,
            default_value: Some("default".to_string()),
            options: vec![],
            required: false,
        };
        let dict = create_form_field_dict(&field);
        assert!(dict.contains("/Type /Annot"));
        assert!(dict.contains("/Subtype /Widget"));
        assert!(dict.contains("/T (username)"));
        assert!(dict.contains("/FT /Tx"));
        assert!(dict.contains("/V (default)"));
        assert!(dict.contains("/Rect [50 600 200 618]"));
    }

    #[test]
    fn test_create_form_field_dict_checkbox() {
        let field = FormField {
            name: "agree".to_string(),
            field_type: FormFieldType::Checkbox,
            x: 50.0,
            y: 550.0,
            width: 15.0,
            height: 15.0,
            default_value: None,
            options: vec![],
            required: true,
        };
        let dict = create_form_field_dict(&field);
        assert!(dict.contains("/FT /Btn"));
        assert!(dict.contains("/T (agree)"));
        assert!(dict.contains("/Ff 2")); // Required flag
        assert!(dict.contains("/V /Off"));
    }

    #[test]
    fn test_create_form_field_dict_dropdown() {
        let field = FormField {
            name: "country".to_string(),
            field_type: FormFieldType::Dropdown,
            x: 50.0,
            y: 500.0,
            width: 100.0,
            height: 20.0,
            default_value: Some("USA".to_string()),
            options: vec!["USA".to_string(), "Canada".to_string(), "Mexico".to_string()],
            required: false,
        };
        let dict = create_form_field_dict(&field);
        assert!(dict.contains("/FT /Ch"));
        assert!(dict.contains("/T (country)"));
        assert!(dict.contains("/V (USA)"));
        assert!(dict.contains("(USA)"));
        assert!(dict.contains("(Canada)"));
        assert!(dict.contains("(Mexico)"));
        assert!(dict.contains("/Ff 131072")); // Combo flag
    }

    #[test]
    fn test_build_text_watermark_positions() {
        let layout = crate::pdf_generator::PageLayout::portrait();

        // Test different positions
        let center_stream = build_text_watermark_stream("TEST", 24.0, 0.5, &layout, WatermarkPosition::Center);
        assert!(String::from_utf8_lossy(&center_stream).contains("(TEST) Tj"));

        let diagonal_stream = build_text_watermark_stream("DRAFT", 48.0, 0.3, &layout, WatermarkPosition::Diagonal);
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

        let result = build_image_watermark_stream(&image_info, 0.5, &layout, WatermarkPosition::Center);
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
    use proptest::prelude::*;
    use super::*;

    proptest! {
        #[test]
        fn merge_metadata_idempotent(base_title in ".*", base_author in ".*",
                                  new_title in ".*", new_author in ".*") {
            let mut base = PdfMetadata::new();
            base.title = Some(base_title);
            base.author = Some(base_author);

            let mut new_meta = PdfMetadata::new();
            new_meta.title = Some(new_title);
            new_meta.author = Some(new_author);

            // Merge twice with same metadata should be idempotent
            let merged1 = merge_metadata(&base, &new_meta);
            let merged2 = merge_metadata(&merged1, &new_meta);

            assert_eq!(merged1.title, merged2.title);
            assert_eq!(merged1.author, merged2.author);
        }
    }

    proptest! {
        #[test]
        fn custom_fields_preserved(key in "[a-zA-Z0-9_]{1,20}", value in ".*") {
            let mut metadata = PdfMetadata::new();
            metadata.add_custom_field(key.clone(), value.clone());

            assert_eq!(metadata.get_custom_field(&key), Some(&value));

            let removed = metadata.remove_custom_field(&key);
            assert_eq!(removed, Some(value));
            assert_eq!(metadata.get_custom_field(&key), None);
        }
    }

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

    #[test]
    fn test_create_portfolio_pdf() {
        let tmp_dir = std::env::temp_dir();
        let file1 = tmp_dir.join("portfolio_test1.txt");
        let file2 = tmp_dir.join("portfolio_test2.csv");
        let output = tmp_dir.join("test_portfolio.pdf");

        std::fs::write(&file1, b"Hello from file 1").unwrap();
        std::fs::write(&file2, b"a,b,c\n1,2,3").unwrap();

        let files = vec![
            (file1.to_string_lossy().to_string(), "Text file".to_string()),
            (file2.to_string_lossy().to_string(), "CSV data".to_string()),
        ];

        create_portfolio_pdf(
            &output.to_string_lossy(),
            &files,
            Some("Test Portfolio"),
        ).unwrap();

        assert!(output.exists(), "Portfolio PDF should be created");

        let bytes = std::fs::read(&output).unwrap();
        let content = String::from_utf8_lossy(&bytes);

        // Should contain Collection dictionary
        assert!(content.contains("/Collection"), "Should contain /Collection");

        // Should contain embedded files
        assert!(content.contains("/EmbeddedFile"), "Should contain /EmbeddedFile");
        assert!(content.contains("/Filespec"), "Should contain /Filespec");

        // Should contain the title
        assert!(content.contains("Test Portfolio"), "Should contain portfolio title");

        // Should be a valid PDF
        assert!(content.starts_with("%PDF-"), "Should be a valid PDF");

        // Cleanup
        let _ = std::fs::remove_file(&file1);
        let _ = std::fs::remove_file(&file2);
        let _ = std::fs::remove_file(&output);
    }
}
