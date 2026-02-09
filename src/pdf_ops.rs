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
/// use pdf_rs::pdf_ops;
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
/// use pdf_rs::pdf_ops;
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
/// use pdf_rs::pdf_ops::PdfMetadata;
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

/// Extract metadata from a PDF document
pub fn extract_metadata_from_pdf(doc: &crate::pdf::PdfDocument) -> Result<PdfMetadata> {
    let mut metadata = PdfMetadata::new();

    // Look for the Info dictionary in the trailer
    // For now, we'll do a simple search for metadata-like objects
    for (_id, obj) in &doc.objects {
        if let crate::pdf::PdfObject::Dictionary(data) = obj {
            // Convert dictionary to a string representation for parsing
            let dict_str = dict_to_string(data);
            if dict_str.contains("/Title") {
                if let Some(title) = extract_pdf_string_field(&dict_str, "/Title") {
                    metadata.title = Some(title);
                }
            }
            if dict_str.contains("/Author") {
                if let Some(author) = extract_pdf_string_field(&dict_str, "/Author") {
                    metadata.author = Some(author);
                }
            }
            if dict_str.contains("/Subject") {
                if let Some(subject) = extract_pdf_string_field(&dict_str, "/Subject") {
                    metadata.subject = Some(subject);
                }
            }
            if dict_str.contains("/Keywords") {
                if let Some(keywords) = extract_pdf_string_field(&dict_str, "/Keywords") {
                    metadata.keywords = Some(keywords);
                }
            }
            if dict_str.contains("/Creator") {
                if let Some(creator) = extract_pdf_string_field(&dict_str, "/Creator") {
                    metadata.creator = Some(creator);
                }
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
                        if let Some(&c) = chars.peek() {
                            if c >= '0' && c <= '7' {
                                chars.next();
                                octal.push(c);
                                if let Some(&c) = chars.peek() {
                                    if c >= '0' && c <= '7' {
                                        chars.next();
                                        octal.push(c);
                                    }
                                }
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
    let page_streams = build_page_streams(&elements, 12.0, true, layout);
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
        generator.add_object(format!("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n"));
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
/// use pdf_rs::pdf_ops;
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
/// use pdf_rs::pdf_ops::{FormField, FormFieldType};
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
    let page_streams = build_page_streams(&elements, 12.0, true, layout);
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
    let pages_obj_id = field_offset + (page_streams.len() as u32) * 3 + 1;
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
        generator.add_object(format!("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n"));
    }

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let pages_dict = format!("<< /Type /Pages\n/Kids [{}]\n/Count {}\n>>\n", kids.join(" "), page_ids.len());
    let actual_pages_id = generator.add_object(pages_dict);
    assert_eq!(actual_pages_id, pages_obj_id);

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
/// use pdf_rs::pdf_ops;
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
        .enumerate()
        .map(|(i, stream)| {
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
/// use pdf_rs::{pdf_ops, security};
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
    // Read the input PDF
    let content = fs::read_to_string(input_file)?;

    // Parse the PDF to find the trailer
    let trailer_pos = content.rfind("trailer")
        .ok_or_else(|| anyhow!("No trailer found in PDF"))?;

    // Create the encryption dictionary
    let encryption_dict = security.create_encryption_dict();

    // If no security is needed, just copy the file
    if !security.is_protected() {
        fs::write(output_file, content)?;
        return Ok(());
    }

    // Insert the encryption dictionary into the PDF
    // We need to add it to the trailer and update the xref table
    // For simplicity, we'll add it as a comment in the output
    let mut protected_content = content.clone();

    // Find the position to insert the encryption dictionary (before the trailer)
    if let Some(trailer_start) = content[trailer_pos..].find("<<") {
        let insert_pos = trailer_pos + trailer_start;

        // Insert the encryption reference
        let encryption_entry = format!("\n/Encrypt {} 0 R\n  ", 1); // Reference to encryption object (we'd add it properly in a full implementation)

        // In a full implementation, we would:
        // 1. Create a new encryption object in the PDF
        // 2. Update the xref table
        // 3. Add the /Encrypt entry to the trailer
        // 4. Encrypt all stream and string objects

        // For this simplified implementation, we'll add a comment indicating protection
        let protection_notice = format!(
            "% PDF PROTECTED: Algorithm={}, Permissions={:08X}\n",
            security.encryption_algorithm.name(),
            security.permissions.to_pdf_flags()
        );

        protected_content.insert_str(0, &protection_notice);

        // Add encryption dictionary reference to trailer (simplified)
        let trailer_with_encrypt = content[insert_pos..].replacen(
            "<<",
            &format!("<<\n/Encrypt <<{}>>", encryption_dict),
            1,
        );

        protected_content = format!(
            "{}{}",
            &protected_content[..insert_pos.min(protected_content.len())],
            trailer_with_encrypt
        );
    }

    // Write the protected PDF
    fs::write(output_file, protected_content)?;

    println!(
        "[protect] Applied protection to {} (algorithm: {})",
        output_file,
        security.encryption_algorithm.name()
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
}
