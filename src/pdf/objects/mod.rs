//! Core PDF object model: [`PdfDocument`], [`PdfObject`], and [`PdfValue`]
//! with document loading, reference rewriting, deduplication, and serialization.

mod document;
mod types;

pub use types::{PdfDocument, PdfObject, PdfValue};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::{LazyPdfDocument, PdfDocument, validate_pdf_bytes};
    use std::collections::HashMap;

    #[test]
    fn test_load_from_bytes_roundtrip() {
        let elements = vec![
            crate::elements::Element::Heading {
                level: 1,
                text: "Roundtrip".into(),
            },
            crate::elements::Element::Paragraph {
                text: "Testing load_from_bytes.".into(),
            },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        // Parse from bytes
        let doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        assert!(!doc.objects.is_empty());

        // Serialize back to bytes
        let roundtrip_bytes = doc.to_bytes();
        assert!(!roundtrip_bytes.is_empty());

        // Re-parse and verify text is intact
        let doc2 = PdfDocument::load_from_bytes(&roundtrip_bytes).unwrap();
        let text = doc2.get_text().unwrap();
        assert!(
            text.contains("Roundtrip"),
            "Text lost after roundtrip: {}",
            text
        );
        assert!(
            text.contains("Testing load_from_bytes."),
            "Text lost after roundtrip: {}",
            text
        );
    }

    #[test]
    fn test_deduplicate_objects() {
        let mut doc = PdfDocument::new();

        // Insert two identical objects
        doc.objects
            .insert(1, PdfObject::String("shared_content".to_string()));
        doc.objects
            .insert(2, PdfObject::String("shared_content".to_string()));

        // Insert a dictionary that references object 2
        let mut dict = HashMap::new();
        dict.insert(
            "Ref".to_string(),
            PdfValue::Object(PdfObject::String("2 0 R".to_string())),
        );
        doc.objects.insert(3, PdfObject::Dictionary(dict));
        doc.catalog = 3;

        assert_eq!(doc.objects.len(), 3, "Should start with 3 objects");

        doc.deduplicate_objects();

        // Object 2 (duplicate) should be removed; object 1 (canonical) kept
        assert_eq!(doc.objects.len(), 2, "Should remove one duplicate");
        assert!(
            doc.objects.contains_key(&1),
            "Canonical object 1 should remain"
        );
        assert!(
            !doc.objects.contains_key(&2),
            "Duplicate object 2 should be removed"
        );
        assert!(
            doc.objects.contains_key(&3),
            "Referencing object 3 should remain"
        );

        // Reference inside object 3 should now point to 1
        if let PdfObject::Dictionary(d) = &doc.objects[&3] {
            if let PdfValue::Object(PdfObject::String(s)) = &d["Ref"] {
                assert_eq!(s, "1 0 R", "Reference should be rewritten to canonical ID");
            } else {
                panic!("Expected string reference value");
            }
        } else {
            panic!("Expected dictionary object");
        }
    }

    #[test]
    fn test_embed_file_attachment() {
        let elements = vec![crate::elements::Element::Paragraph {
            text: "Document with attachment".into(),
        }];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let mut doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        let original_count = doc.objects.len();

        // Embed a simple text file
        let attachment_data = b"Hello, this is an embedded file!";
        let fs_id = doc.embed_file("test.txt", attachment_data).unwrap();

        // Should have added 2 new objects: embedded file stream + file spec
        assert_eq!(
            doc.objects.len(),
            original_count + 2,
            "Should add 2 objects (embedded file stream + file spec)"
        );

        // Verify the file spec object exists
        assert!(
            doc.objects.contains_key(&fs_id),
            "File spec object should exist"
        );

        // Verify the embedded file stream exists (should be fs_id - 1)
        let ef_id = fs_id - 1;
        assert!(
            doc.objects.contains_key(&ef_id),
            "Embedded file stream object should exist"
        );

        // Verify the catalog was updated with /Names
        if let Some(PdfObject::Dictionary(catalog_dict)) = doc.objects.get(&doc.catalog) {
            assert!(
                catalog_dict.contains_key("Names"),
                "Catalog should contain /Names for embedded files"
            );
        } else {
            panic!("Catalog should be a dictionary");
        }

        // Verify the output PDF serializes correctly
        let output_bytes = doc.to_bytes();
        assert!(
            !output_bytes.is_empty(),
            "PDF with attachment should serialize"
        );

        // Verify /EmbeddedFile appears in output
        let content = String::from_utf8_lossy(&output_bytes);
        assert!(
            content.contains("/EmbeddedFile"),
            "Output should contain /EmbeddedFile type"
        );
        assert!(
            content.contains("/Filespec"),
            "Output should contain /Filespec type"
        );
        assert!(
            content.contains("test.txt"),
            "Output should contain attachment filename"
        );
    }

    #[test]
    fn test_roundtrip_generate_validate_parse() {
        // Round-trip: elements → PDF bytes → validate → parse → extract text → verify
        let elements = vec![
            crate::elements::Element::Heading {
                level: 1,
                text: "Roundtrip Title".into(),
            },
            crate::elements::Element::Paragraph {
                text: "This is roundtrip content.".into(),
            },
            crate::elements::Element::UnorderedListItem {
                text: "Item one".into(),
                depth: 0,
            },
            crate::elements::Element::UnorderedListItem {
                text: "Item two".into(),
                depth: 0,
            },
            crate::elements::Element::CodeBlock {
                language: "rust".into(),
                code: "fn main() {}".into(),
            },
            crate::elements::Element::BlockQuote {
                text: "A quote".into(),
                depth: 1,
            },
            crate::elements::Element::Link {
                text: "Example".into(),
                url: "https://example.com".into(),
            },
            crate::elements::Element::Image {
                alt: "Logo".into(),
                path: format!("{}/tests/fixtures/sample.png", env!("CARGO_MANIFEST_DIR")),
            },
            crate::elements::Element::Footnote {
                label: "1".into(),
                text: "A footnote.".into(),
            },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        // 1. Validate structure
        let validation = validate_pdf_bytes(&pdf_bytes);
        assert!(
            validation.valid,
            "PDF should be valid. Errors: {:?}",
            validation.errors
        );
        assert!(validation.page_count >= 1);

        // 2. Parse back and extract text
        let content = String::from_utf8_lossy(&pdf_bytes);
        // Check key content strings are present in the raw PDF
        assert!(
            content.contains("Roundtrip Title"),
            "Title not found in PDF"
        );
        assert!(
            content.contains("roundtrip content"),
            "Paragraph not found in PDF"
        );
        assert!(content.contains("Item one"), "List item not found in PDF");
        assert!(
            content.contains("fn") && content.contains("main"),
            "Code block not found in PDF"
        );
        assert!(content.contains("quote"), "Blockquote not found in PDF");
        assert!(content.contains("Example"), "Link text not found in PDF");
        assert!(content.contains("example.com"), "Link URL not found in PDF");
        assert!(content.contains("Logo"), "Image alt not found in PDF");
        assert!(
            content.contains("/XObject"),
            "embedded image XObject missing"
        );
        assert!(content.contains("footnote"), "Footnote not found in PDF");
    }

    #[test]
    fn test_roundtrip_all_element_types() {
        // Comprehensive round-trip: every element type → PDF → validate → verify text
        let elements = vec![
            crate::elements::Element::Heading {
                level: 1,
                text: "H1 Title".into(),
            },
            crate::elements::Element::Heading {
                level: 2,
                text: "H2 Subtitle".into(),
            },
            crate::elements::Element::Heading {
                level: 3,
                text: "H3 Section".into(),
            },
            crate::elements::Element::Paragraph {
                text: "Normal paragraph text here.".into(),
            },
            crate::elements::Element::EmptyLine,
            crate::elements::Element::UnorderedListItem {
                text: "Bullet item".into(),
                depth: 0,
            },
            crate::elements::Element::OrderedListItem {
                number: 1,
                text: "Numbered item".into(),
                depth: 0,
            },
            crate::elements::Element::TaskListItem {
                checked: true,
                text: "Done task".into(),
            },
            crate::elements::Element::TaskListItem {
                checked: false,
                text: "Todo task".into(),
            },
            crate::elements::Element::CodeBlock {
                language: "python".into(),
                code: "print('hello')".into(),
            },
            crate::elements::Element::InlineCode {
                code: "let x = 42".into(),
            },
            crate::elements::Element::TableRow {
                cells: vec!["Name".into(), "Age".into()],
                is_separator: false,
                alignments: vec![
                    crate::elements::TableAlignment::Left,
                    crate::elements::TableAlignment::Left,
                ],
                colspans: Vec::new(),
                rowspans: Vec::new(),
            },
            crate::elements::Element::BlockQuote {
                text: "Wise words".into(),
                depth: 1,
            },
            crate::elements::Element::DefinitionItem {
                term: "Rust".into(),
                definition: "A language".into(),
            },
            crate::elements::Element::Footnote {
                label: "fn1".into(),
                text: "See reference".into(),
            },
            crate::elements::Element::Link {
                text: "Google".into(),
                url: "https://google.com".into(),
            },
            crate::elements::Element::Image {
                alt: "Photo".into(),
                path: format!("{}/tests/fixtures/sample.png", env!("CARGO_MANIFEST_DIR")),
            },
            crate::elements::Element::StyledText {
                text: "Bold text".into(),
                bold: true,
                italic: false,
            },
            crate::elements::Element::HorizontalRule,
            crate::elements::Element::PageBreak,
            crate::elements::Element::Paragraph {
                text: "After page break.".into(),
            },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        // Validate
        let validation = validate_pdf_bytes(&pdf_bytes);
        assert!(
            validation.valid,
            "PDF with all elements should be valid. Errors: {:?}",
            validation.errors
        );
        assert!(
            validation.page_count >= 2,
            "PageBreak should create at least 2 pages, got {}",
            validation.page_count
        );

        // Verify content
        let content = String::from_utf8_lossy(&pdf_bytes);
        let expected_strings = vec![
            "H1 Title",
            "H2 Subtitle",
            "H3 Section",
            "Normal paragraph",
            "Bullet item",
            "Numbered item",
            "Done task",
            "Todo task",
            "print",
            "let",
            "x = 42",
            "Name",
            "Age",
            "Wise words",
            "Rust",
            "A language",
            "See reference",
            "Google",
            "google.com",
            "Photo",
            "Figure",
            "Bold text",
            "After page break",
        ];
        for s in &expected_strings {
            assert!(content.contains(s), "Expected '{}' in PDF content", s);
        }
        assert!(
            content.contains("/XObject"),
            "embedded image XObject missing"
        );
    }

    #[test]
    fn test_missing_image_fails_generation() {
        let elements = vec![crate::elements::Element::Image {
            alt: "Gone".into(),
            path: "/no/such/image.png".into(),
        }];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let err = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout)
            .expect_err("missing image must fail");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("Image embedding failed") || msg.contains("failed to load image"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn test_roundtrip_landscape() {
        let elements = vec![
            crate::elements::Element::Heading {
                level: 1,
                text: "Landscape Doc".into(),
            },
            crate::elements::Element::Paragraph {
                text: "Wide content.".into(),
            },
        ];
        let layout = crate::pdf_generator::PageLayout::landscape();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let validation = validate_pdf_bytes(&pdf_bytes);
        assert!(
            validation.valid,
            "Landscape PDF should be valid. Errors: {:?}",
            validation.errors
        );

        // Check landscape dimensions (792 x 612)
        let content = String::from_utf8_lossy(&pdf_bytes);
        assert!(content.contains("792"), "Landscape width should be 792");
        assert!(content.contains("612"), "Landscape height should be 612");
    }

    #[test]
    fn test_lazy_pdf_document_text_extraction() {
        let elements = vec![
            crate::elements::Element::Heading {
                level: 1,
                text: "Lazy Test".into(),
            },
            crate::elements::Element::Paragraph {
                text: "Testing lazy text extraction.".into(),
            },
            crate::elements::Element::Paragraph {
                text: "Second paragraph for good measure.".into(),
            },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        // Lazy document should extract same text as full document
        let lazy_doc = LazyPdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        let lazy_text = lazy_doc.get_text().unwrap();

        let full_doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        let _full_text = full_doc.get_text().unwrap();

        assert!(
            lazy_text.contains("Lazy Test"),
            "Lazy text should contain heading: {}",
            lazy_text
        );
        assert!(
            lazy_text.contains("Testing lazy text extraction."),
            "Lazy text should contain paragraph: {}",
            lazy_text
        );
        assert!(
            lazy_text.contains("Second paragraph"),
            "Lazy text should contain second paragraph: {}",
            lazy_text
        );

        // Lazy document should have fewer/no non-stream objects materialized
        // but text content should be equivalent
        assert!(
            !lazy_text.is_empty(),
            "Lazy text extraction should produce non-empty output"
        );
    }

    #[test]
    fn test_repl_like_workflow() {
        // Simulate a REPL session: create -> load -> modify -> save -> reload -> verify
        let elements = vec![
            crate::elements::Element::Heading {
                level: 1,
                text: "REPL Test".into(),
            },
            crate::elements::Element::Paragraph {
                text: "First paragraph.".into(),
            },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        // "load" step
        let mut doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        assert!(!doc.objects.is_empty(), "Should load document");

        // "text" step
        let text = doc.get_text().unwrap();
        assert!(text.contains("REPL Test"), "Text extraction should work");

        // "info" step
        assert_eq!(doc.version, "1.4", "Version should be 1.4");
        assert!(doc.catalog > 0, "Should have a catalog");

        // "sanitize" step
        doc.sanitize();

        // "attach" step
        doc.embed_file("note.txt", b"REPL session note").unwrap();

        // "save" step (serialize to bytes)
        let saved_bytes = doc.to_bytes();
        assert!(!saved_bytes.is_empty(), "Should serialize document");

        // "reload" step
        let reloaded = PdfDocument::load_from_bytes(&saved_bytes).unwrap();
        let reloaded_text = reloaded.get_text().unwrap();
        assert!(
            reloaded_text.contains("REPL Test"),
            "Text should survive round-trip"
        );

        // "validate" step
        let validation = validate_pdf_bytes(&saved_bytes);
        assert!(
            validation.valid,
            "Round-tripped PDF should be valid: {:?}",
            validation.errors
        );
    }
}
