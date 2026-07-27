//! PDF metadata creation, extraction, and merging.

use anyhow::Result;
use std::fs;

use crate::pdf::{PdfDocument, PdfObject, PdfValue};

/// PDF document metadata (Info dictionary fields).
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
            entries.push(format!("/Title ({})", super::escape_pdf_meta(t)));
        }
        if let Some(ref a) = self.author {
            entries.push(format!("/Author ({})", super::escape_pdf_meta(a)));
        }
        if let Some(ref s) = self.subject {
            entries.push(format!("/Subject ({})", super::escape_pdf_meta(s)));
        }
        if let Some(ref k) = self.keywords {
            entries.push(format!("/Keywords ({})", super::escape_pdf_meta(k)));
        }
        if let Some(ref c) = self.creator {
            entries.push(format!("/Creator ({})", super::escape_pdf_meta(c)));
        }
        entries.push("/Producer (pdf-cli)".to_string());

        // Add custom fields
        for (key, value) in &self.custom_fields {
            // Escape the key as well (though typically keys are simple strings)
            let escaped_key = super::escape_pdf_meta(key);
            let escaped_value = super::escape_pdf_meta(value);
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
    let page_streams = super::build_page_streams(
        elements,
        base_font_size,
        show_page_numbers,
        layout,
        image_base_dir,
    )?;

    assemble_pdf_with_metadata(filename, &page_streams, font, &layout, metadata)?;
    Ok(())
}

/// Assemble PDF with optional metadata Info dictionary
pub(super) fn assemble_pdf_with_metadata(
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

        let font_dict = format!("<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n", font);
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

/// Extract metadata from a PDF document
pub fn extract_metadata_from_pdf(doc: &PdfDocument) -> Result<PdfMetadata> {
    let mut metadata = PdfMetadata::new();

    // Look for the Info dictionary in the trailer
    // For now, we'll do a simple search for metadata-like objects
    for obj in doc.objects.values() {
        if let PdfObject::Dictionary(data) = obj {
            // Convert dictionary to a string representation for parsing
            let dict_str = dict_to_string(data);
            if dict_str.contains("/Title")
                && let Some(title) = extract_pdf_string_field(&dict_str, "/Title")
            {
                metadata.title = Some(title);
            }
            if dict_str.contains("/Author")
                && let Some(author) = extract_pdf_string_field(&dict_str, "/Author")
            {
                metadata.author = Some(author);
            }
            if dict_str.contains("/Subject")
                && let Some(subject) = extract_pdf_string_field(&dict_str, "/Subject")
            {
                metadata.subject = Some(subject);
            }
            if dict_str.contains("/Keywords")
                && let Some(keywords) = extract_pdf_string_field(&dict_str, "/Keywords")
            {
                metadata.keywords = Some(keywords);
            }
            if dict_str.contains("/Creator")
                && let Some(creator) = extract_pdf_string_field(&dict_str, "/Creator")
            {
                metadata.creator = Some(creator);
            }
        }
    }

    Ok(metadata)
}

/// Convert a PDF dictionary HashMap to a string representation
fn dict_to_string(dict: &std::collections::HashMap<String, PdfValue>) -> String {
    let mut parts = Vec::new();
    for (key, value) in dict {
        parts.push(format!("/{} {}", key, value_to_string(value)));
    }
    parts.join(" ")
}

/// Convert a PdfValue to its string representation
fn value_to_string(value: &PdfValue) -> String {
    match value {
        PdfValue::Object(obj) => object_to_string(obj),
        PdfValue::Reference(id, generation) => format!("{} {} R", id, generation),
    }
}

/// Convert a PdfObject to its string representation
fn object_to_string(obj: &PdfObject) -> String {
    match obj {
        PdfObject::Dictionary(dict) => {
            let entries: Vec<String> = dict
                .iter()
                .map(|(k, v)| format!("/{} {}", k, value_to_string(v)))
                .collect();
            format!("<< {} >>", entries.join(" "))
        }
        PdfObject::Stream {
            dictionary: _,
            data: _,
        } => "<< stream >>".to_string(),
        PdfObject::Array(arr) => {
            let elems: Vec<String> = arr.iter().map(value_to_string).collect();
            format!("[{}]", elems.join(" "))
        }
        PdfObject::String(s) => format!("({})", super::escape_pdf_meta(s)),
        PdfObject::Number(n) => n.to_string(),
        PdfObject::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
        PdfObject::Null => "null".to_string(),
        PdfObject::Reference(id, generation) => format!("{} {} R", id, generation),
        PdfObject::Name(n) => format!("/{}", n),
    }
}

/// Extract a string field value from PDF dictionary content
pub(super) fn extract_pdf_string_field(content: &str, field: &str) -> Option<String> {
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
pub(super) fn unescape_pdf_string(s: &str) -> String {
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
                            && ('0'..='7').contains(&c)
                        {
                            chars.next();
                            octal.push(c);
                            if let Some(&c) = chars.peek()
                                && ('0'..='7').contains(&c)
                            {
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
    fn test_pdf_metadata_default() {
        let meta = PdfMetadata::new();
        assert!(meta.title.is_none());
        assert!(meta.author.is_none());
        let dict = meta.to_info_dict();
        assert!(dict.contains("/Producer (pdf-cli)"));
    }

    #[test]
    fn test_custom_metadata_fields() {
        let mut metadata = PdfMetadata::new();
        metadata.add_custom_field("CustomField1".to_string(), "Value1".to_string());
        metadata.add_custom_field("CustomField2".to_string(), "Value2".to_string());

        assert_eq!(
            metadata.get_custom_field("CustomField1"),
            Some(&"Value1".to_string())
        );
        assert_eq!(
            metadata.get_custom_field("CustomField2"),
            Some(&"Value2".to_string())
        );
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
        assert_eq!(
            merged.get_custom_field("BaseField"),
            Some(&"BaseValue".to_string())
        ); // Preserved
        assert_eq!(
            merged.get_custom_field("NewField"),
            Some(&"NewValue".to_string())
        ); // Added
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
        assert_eq!(
            extract_pdf_string_field(content, "/Title"),
            Some("Test Title".to_string())
        );
        assert_eq!(
            extract_pdf_string_field(content, "/Author"),
            Some("Test (Author) ".to_string())
        );
        assert_eq!(extract_pdf_string_field(content, "/Subject"), None);
        assert_eq!(extract_pdf_string_field(content, "/NonExistent"), None);
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

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
}
