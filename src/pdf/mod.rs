//! PDF document model, parsing, text extraction, and structural diff
//!
//! Provides [`PdfDocument`] for loading PDFs from files or bytes,
//! navigating objects, extracting text with [`PdfDocument::get_text`], validating
//! structure, and comparing documents with [`diff_pdf_bytes`].

use std::sync::OnceLock;

macro_rules! pdf_regex {
    ($name:ident, $pat:literal) => {
        fn $name() -> &'static regex::Regex {
            static RE: OnceLock<regex::Regex> = OnceLock::new();
            RE.get_or_init(|| regex::Regex::new($pat).unwrap())
        }
    };
}

pdf_regex!(re_root, r"/Root\s+(\d+)\s+\d+\s+R");
pdf_regex!(re_root_any, r"/Root\s+\d+\s+\d+\s+R");
pdf_regex!(re_obj_ref, r"^(\d+) (\d+) R$");
pdf_regex!(re_obj, r"(\d+)\s+(\d+)\s+obj\b");
pdf_regex!(re_obj_count, r"\d+\s+\d+\s+obj\b");
pdf_regex!(re_tj, r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)\s*Tj");
pdf_regex!(re_tj_hex, r"<([0-9a-fA-F\s]+)>\s*Tj");
pdf_regex!(re_tj_array, r"\[((?:[^\]]*?))\]\s*TJ");
pdf_regex!(re_tj_str, r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)");
pdf_regex!(re_tj_hex_str, r"<([0-9a-fA-F\s]+)>");
pdf_regex!(re_td, r"([\d.\-]+)\s+([\d.\-]+)\s+T[dD]");
pdf_regex!(
    re_tm,
    r"[\d.\-]+\s+[\d.\-]+\s+[\d.\-]+\s+[\d.\-]+\s+([\d.\-]+)\s+([\d.\-]+)\s+Tm"
);
pdf_regex!(re_page, r"/Type\s+/Page[^s]");
pdf_regex!(re_page_eol, r"/Type\s+/Page\s*\n");

mod decode;
mod diff;
mod objects;
mod parser;
mod sandbox;
mod text_extract;
/// Structural and compliance validation (PDF, PDF/A, PDF/UA, screen reader).
///
/// Public items are re-exported below so existing `crate::pdf::validate_*`
/// call sites continue to work without changes.
mod validation;

pub(crate) use decode::decode_pdf_hex_string_with_map;
pub use decode::{decode_pdf_hex_string, decode_with_encoding, unescape_pdf_string};
pub use diff::{PdfDiff, diff_pdf_bytes};
pub use objects::{PdfDocument, PdfObject, PdfValue};
pub use parser::{LazyPdfDocument, parse_object_stream, parse_xref_stream};
pub use sandbox::{JavaScriptAction, JavaScriptSandboxReport, sandbox_pdf_bytes};
pub(crate) use text_extract::collect_tounicode_gid_map;
pub use text_extract::extract_text;
pub use validation::{
    PdfAValidation, PdfUaValidation, PdfValidation, ScreenReaderComplianceReport,
    check_screen_reader_compliance, check_screen_reader_compliance_bytes, validate_pdf,
    validate_pdf_a, validate_pdf_a_bytes, validate_pdf_a3, validate_pdf_a3_bytes,
    validate_pdf_bytes, validate_pdf_ua, validate_pdf_ua_bytes,
};

#[cfg(test)]
mod tests {
    use super::decode::{
        TextPositionTracker, decode_pdf_hex_string, decode_utf16be, decode_with_encoding,
        macroman_decode, unescape_pdf_string, winansi_decode,
    };
    use super::parser::{
        decompress_stream, parse_dict_entries, parse_object_stream, parse_xref_stream,
        read_xref_field,
    };
    use super::text_extract::resolve_unicode_ttf_path_for_extraction;
    use super::*;
    use std::collections::HashMap;
    use std::fs;

    #[test]
    fn test_unescape_pdf_string() {
        assert_eq!(unescape_pdf_string(r"hello"), "hello");
        assert_eq!(unescape_pdf_string(r"hello\nworld"), "hello\nworld");
        assert_eq!(unescape_pdf_string(r"a\(b\)c"), "a(b)c");
        assert_eq!(unescape_pdf_string(r"back\\slash"), "back\\slash");
        assert_eq!(unescape_pdf_string(r"tab\there"), "tab\there");
        assert_eq!(unescape_pdf_string(r"form\ffeed"), "form\u{000C}feed");
        assert_eq!(unescape_pdf_string(r"back\bspace"), "back\u{0008}space");
    }

    #[test]
    fn test_unescape_octal_sequences() {
        assert_eq!(unescape_pdf_string(r"\101"), "A");
        assert_eq!(unescape_pdf_string(r"\101\102\103"), "ABC");
        assert_eq!(unescape_pdf_string(r"\60"), "0");
        assert_eq!(unescape_pdf_string(r"\141\142\143"), "abc");
        assert_eq!(unescape_pdf_string(r"Hello\40World"), "Hello World");
    }

    #[test]
    fn test_decode_hex_string_basic() {
        assert_eq!(decode_pdf_hex_string("48656C6C6F"), "Hello");
        assert_eq!(decode_pdf_hex_string("576F726C64"), "World");
        assert_eq!(decode_pdf_hex_string("414243"), "ABC");
        assert_eq!(decode_pdf_hex_string("48 65 6C 6C 6F"), "Hello");
    }

    #[test]
    fn test_decode_hex_string_utf16be() {
        assert_eq!(decode_pdf_hex_string("FEFF00480065006C006C006F"), "Hello");
        assert_eq!(decode_pdf_hex_string("FEFF4F60597D"), "你好");
        assert_eq!(decode_pdf_hex_string("FEFF0041004200430044"), "ABCD");
    }

    #[test]
    fn test_decode_hex_string_unicode_symbols() {
        assert_eq!(decode_pdf_hex_string("FEFF03B103B203B3"), "αβγ");
        assert_eq!(decode_pdf_hex_string("FEFF221E2211222B"), "∞∑∫");
    }

    #[test]
    fn test_decode_hex_string_unicode_glyph_ids_roundtrip() {
        let Some(path) = resolve_unicode_ttf_path_for_extraction() else {
            return;
        };
        let Ok(bytes) = fs::read(path) else {
            return;
        };
        let Ok(face) = ttf_parser::Face::parse(&bytes, 0) else {
            return;
        };

        let sample = "Unicode test: 你好 Γεια ∑";
        let mut encoded = String::new();
        for ch in sample.chars() {
            let Some(gid) = face.glyph_index(ch) else {
                return;
            };
            encoded.push_str(&format!("{:04X}", gid.0));
        }

        assert_eq!(decode_pdf_hex_string(&encoded), sample);
    }

    #[test]
    fn test_decode_utf16be_surrogate_pairs() {
        let bytes = vec![0xD8, 0x3D, 0xDE, 0x00];
        assert_eq!(decode_utf16be(&bytes), "😀");

        let bytes2 = vec![0xD8, 0x3D, 0xDE, 0x01];
        assert_eq!(decode_utf16be(&bytes2), "😁");
    }

    #[test]
    fn test_winansi_decode() {
        assert_eq!(winansi_decode(0x41), 'A');
        assert_eq!(winansi_decode(0x80), '\u{20AC}'); // Euro
        assert_eq!(winansi_decode(0x95), '\u{2022}'); // Bullet
        assert_eq!(winansi_decode(0x96), '\u{2013}'); // En dash
        assert_eq!(winansi_decode(0x97), '\u{2014}'); // Em dash
    }

    #[test]
    fn test_macroman_decode() {
        assert_eq!(macroman_decode(0x41), 'A');
        assert_eq!(macroman_decode(0x80), '\u{00C4}'); // Ä
        assert_eq!(macroman_decode(0x8A), '\u{00E4}'); // ä (index 10 in high table)
    }

    #[test]
    fn test_decode_with_encoding() {
        let data = b"Hello";
        assert_eq!(decode_with_encoding(data, "WinAnsiEncoding"), "Hello");
        assert_eq!(decode_with_encoding(data, "MacRomanEncoding"), "Hello");
        assert_eq!(decode_with_encoding(data, "StandardEncoding"), "Hello");
    }

    #[test]
    fn test_parse_dict_entries() {
        let raw = "<< /Type /Page /Length 42 >>";
        let dict = parse_dict_entries(raw);
        assert!(dict.contains_key("Type"));
        assert!(dict.contains_key("Length"));
    }

    #[test]
    fn test_text_position_tracker() {
        let mut tracker = TextPositionTracker::new();
        assert!(!tracker.moved_to_new_line(720.0)); // first call, no previous
        assert!(!tracker.moved_to_new_line(720.0)); // same Y
        assert!(tracker.moved_to_new_line(700.0)); // moved 20 units
        assert!(!tracker.moved_to_new_line(700.0)); // same Y again
    }

    #[test]
    fn test_decompress_stream_passthrough() {
        let data = b"BT /F1 12 Tf (Hello) Tj ET";
        let result = decompress_stream(data);
        assert_eq!(result, data);
    }

    #[test]
    fn test_read_xref_field() {
        // 1-byte field
        assert_eq!(read_xref_field(&[0x01], 0, 1), 1);
        assert_eq!(read_xref_field(&[0xFF], 0, 1), 255);

        // 2-byte field (big-endian)
        assert_eq!(read_xref_field(&[0x01, 0x00], 0, 2), 256);
        assert_eq!(read_xref_field(&[0x00, 0x2A], 0, 2), 42);

        // 3-byte field
        assert_eq!(read_xref_field(&[0x01, 0x00, 0x00], 0, 3), 65536);

        // 0-width field
        assert_eq!(read_xref_field(&[0xFF], 0, 0), 0);
    }

    #[test]
    fn test_parse_xref_stream_basic() {
        // W = [1, 2, 1], size = 3
        // Entry 0: type=0, offset=0x0000, gen=0xFF (free)
        // Entry 1: type=1, offset=0x0100, gen=0x00 (normal at offset 256)
        // Entry 2: type=2, offset=0x0005, gen=0x02 (compressed in obj 5, index 2)
        let data: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0xFF, // entry 0: type=0, field2=0, field3=255
            0x01, 0x01, 0x00, 0x00, // entry 1: type=1, field2=256, field3=0
            0x02, 0x00, 0x05, 0x02, // entry 2: type=2, field2=5, field3=2
        ];
        let w = vec![1, 2, 1];
        let entries = parse_xref_stream(&data, &w, 3);

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0], (0, 0, 255));
        assert_eq!(entries[1], (1, 256, 0));
        assert_eq!(entries[2], (2, 5, 2));
    }

    #[test]
    fn test_parse_xref_stream_empty() {
        let entries = parse_xref_stream(&[], &[1, 2, 1], 0);
        assert!(entries.is_empty());

        let entries = parse_xref_stream(&[0x01], &[], 1);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_object_stream() {
        // Object stream with 2 objects:
        // Header: "10 0 20 14 " (obj 10 at offset 0, obj 20 at offset 14)
        // First = 10 (offset where object data starts)
        // Data after first: "<< /Type /Page >>null"
        let stream = b"10 0 20 14 << /Type /Page >>null";
        let first = 11; // "10 0 20 14 " is 11 bytes
        let results = parse_object_stream(stream, 2, first);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 10); // obj num
        assert!(results[0].1.contains("/Type"));
        assert_eq!(results[1].0, 20); // obj num
    }

    #[test]
    fn test_parse_object_stream_empty() {
        let results = parse_object_stream(b"", 0, 0);
        assert!(results.is_empty());

        // first beyond data length
        let results = parse_object_stream(b"10 0 ", 1, 100);
        assert!(results.is_empty());
    }

    #[test]
    fn test_validate_pdf_bytes_valid() {
        // Generate a valid PDF via the library
        let elements = vec![
            crate::elements::Element::Heading {
                level: 1,
                text: "Test Title".into(),
            },
            crate::elements::Element::Paragraph {
                text: "Hello world paragraph.".into(),
            },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let result = validate_pdf_bytes(&pdf_bytes);
        assert!(
            result.valid,
            "Generated PDF should be valid. Errors: {:?}",
            result.errors
        );
        assert!(result.page_count >= 1, "Should have at least 1 page");
        assert!(result.object_count > 0, "Should have objects");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_pdf_bytes_invalid_header() {
        let result = validate_pdf_bytes(b"NOT A PDF FILE");
        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("Missing PDF header"))
        );
    }

    #[test]
    fn test_validate_pdf_bytes_empty() {
        let result = validate_pdf_bytes(b"");
        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("Missing PDF header"))
        );
    }

    #[test]
    fn test_validate_pdf_bytes_missing_eof() {
        let result = validate_pdf_bytes(b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n");
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("%%EOF")));
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
    fn test_validate_pdf_a_generated_pdf() {
        let elements = vec![
            crate::elements::Element::Heading {
                level: 1,
                text: "PDF/A Test".into(),
            },
            crate::elements::Element::Paragraph {
                text: "Testing PDF/A validation.".into(),
            },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let result = validate_pdf_a_bytes(&pdf_bytes);
        // Generated PDFs use Base-14 fonts without embedding, so they won't be fully PDF/A compliant
        // but should have no encryption, no JS, no external references
        assert!(
            !result.has_encryption,
            "Generated PDF should not have encryption"
        );
        assert!(
            !result.errors.iter().any(|e| e.contains("JavaScript")),
            "No JS expected"
        );
        assert!(
            !result.errors.iter().any(|e| e.contains("external")),
            "No external refs expected"
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
    fn test_validate_pdf_a3_fails_without_embedded_files() {
        let elements = vec![crate::elements::Element::Paragraph {
            text: "No attachments".into(),
        }];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let result = validate_pdf_a3_bytes(&pdf_bytes);
        assert!(
            result.errors.iter().any(|e| e.contains("embedded file")),
            "PDF/A-3 should fail without embedded files: {:?}",
            result.errors
        );
        assert!(
            !result.compliant,
            "Should not be PDF/A-3 compliant without attachments"
        );
    }

    #[test]
    fn test_validate_pdf_a3_passes_with_embedded_files() {
        let elements = vec![crate::elements::Element::Paragraph {
            text: "With attachment".into(),
        }];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let mut doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        doc.embed_file("data.csv", b"a,b,c\n1,2,3").unwrap();
        let output_bytes = doc.to_bytes();

        let result = validate_pdf_a3_bytes(&output_bytes);
        // Still may fail on other PDF/A checks (fonts, XMP) but should NOT fail on embedded files
        assert!(
            !result.errors.iter().any(|e| e.contains("embedded file")),
            "PDF/A-3 should not complain about embedded files when present: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_validate_pdf_ua_detects_missing_accessibility() {
        let elements = vec![crate::elements::Element::Paragraph {
            text: "Untagged doc".into(),
        }];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let result = validate_pdf_ua_bytes(&pdf_bytes);
        // Our generated PDFs don't have MarkInfo, StructTreeRoot, Lang, or Title yet
        assert!(
            !result.compliant,
            "Untagged PDF should not be PDF/UA compliant"
        );
        assert!(!result.has_mark_info, "Should detect missing MarkInfo");
        assert!(
            !result.has_struct_tree,
            "Should detect missing StructTreeRoot"
        );
        assert!(!result.has_lang, "Should detect missing Lang");
        assert!(!result.has_title, "Should detect missing Title");
    }

    #[test]
    fn test_check_screen_reader_compliance_tagged_vs_untagged() {
        let layout = crate::pdf_generator::PageLayout::portrait();
        let elements = vec![
            crate::elements::Element::Heading {
                level: 1,
                text: "Accessible".into(),
            },
            crate::elements::Element::Paragraph {
                text: "Screen reader test content.".into(),
            },
        ];

        let untagged =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();
        let untagged_report = check_screen_reader_compliance_bytes(&untagged);
        assert!(
            !untagged_report.compliant,
            "Untagged PDF should fail screen reader compliance"
        );
        assert!(!untagged_report.issues.is_empty());

        let opts = crate::pdf_generator::AccessibilityOptions::new()
            .with_tagged_pdf(true)
            .with_language("en-US".to_string())
            .with_title("Accessible Doc".to_string());
        let tagged = crate::pdf_generator::generate_tagged_pdf_bytes(
            &elements,
            "Helvetica",
            12.0,
            layout,
            opts,
        )
        .unwrap();
        let tagged_report = check_screen_reader_compliance_bytes(&tagged);
        assert!(
            tagged_report.compliant,
            "Tagged PDF should pass: {:?}",
            tagged_report.issues
        );
        assert!(tagged_report.text_extractable);
        assert!(
            tagged_report
                .structure_element_types
                .contains(&"Document".to_string())
        );
    }

    #[test]
    fn test_sanitize_removes_dangerous_objects() {
        let elements = vec![crate::elements::Element::Paragraph {
            text: "Safe document".into(),
        }];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let mut doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        let original_count = doc.objects.len();

        // Inject a fake JavaScript object
        let mut js_dict = HashMap::new();
        js_dict.insert(
            "JS".to_string(),
            PdfValue::Object(PdfObject::String("app.alert('xss')".to_string())),
        );
        doc.objects.insert(999, PdfObject::Dictionary(js_dict));

        // Inject a fake launch action
        let mut launch_dict = HashMap::new();
        launch_dict.insert(
            "S".to_string(),
            PdfValue::Object(PdfObject::String("/Launch".to_string())),
        );
        launch_dict.insert(
            "F".to_string(),
            PdfValue::Object(PdfObject::String("(malware.exe)".to_string())),
        );
        doc.objects.insert(998, PdfObject::Dictionary(launch_dict));

        // Add OpenAction to catalog
        if let Some(PdfObject::Dictionary(catalog_dict)) = doc.objects.get_mut(&doc.catalog) {
            catalog_dict.insert(
                "OpenAction".to_string(),
                PdfValue::Object(PdfObject::String("999 0 R".to_string())),
            );
        }

        assert_eq!(
            doc.objects.len(),
            original_count + 2,
            "Should have injected 2 dangerous objects"
        );

        // Sanitize
        doc.sanitize();

        // JS object should be removed entirely
        assert!(
            !doc.objects.contains_key(&999),
            "JavaScript object should be removed"
        );

        // Launch action should be removed entirely
        assert!(
            !doc.objects.contains_key(&998),
            "Launch action object should be removed"
        );

        // Catalog should no longer have OpenAction
        if let Some(PdfObject::Dictionary(catalog_dict)) = doc.objects.get(&doc.catalog) {
            assert!(
                !catalog_dict.contains_key("OpenAction"),
                "OpenAction should be stripped from catalog"
            );
        } else {
            panic!("Catalog should remain a dictionary");
        }

        // Safe objects should still be present
        assert_eq!(
            doc.objects.len(),
            original_count,
            "Only dangerous objects should be removed"
        );

        // Verify PDF still serializes correctly
        let output_bytes = doc.to_bytes();
        assert!(
            !output_bytes.is_empty(),
            "Sanitized PDF should still serialize"
        );
        let content = String::from_utf8_lossy(&output_bytes);
        assert!(
            !content.contains("app.alert"),
            "JS payload should not remain in output"
        );
    }

    #[test]
    fn test_javascript_sandbox_detects_and_strips_actions() {
        let elements = vec![crate::elements::Element::Paragraph {
            text: "Sandbox test".into(),
        }];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let mut doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();

        let mut js_action = HashMap::new();
        js_action.insert(
            "S".to_string(),
            PdfValue::Object(PdfObject::String("/JavaScript".to_string())),
        );
        js_action.insert(
            "JS".to_string(),
            PdfValue::Object(PdfObject::String("app.alert('x')".to_string())),
        );

        let mut annot = HashMap::new();
        annot.insert(
            "A".to_string(),
            PdfValue::Object(PdfObject::Dictionary(js_action)),
        );
        doc.objects.insert(997, PdfObject::Dictionary(annot));

        let mut uri_action = HashMap::new();
        uri_action.insert(
            "URI".to_string(),
            PdfValue::Object(PdfObject::String("(javascript:alert(1))".to_string())),
        );
        doc.objects.insert(996, PdfObject::Dictionary(uri_action));

        let before = doc.detect_javascript_actions();
        assert!(
            before.actions_found.len() >= 2,
            "Should detect JS action and javascript: URI"
        );

        let report = doc.sandbox();
        assert!(report.clean, "Sandboxed PDF should have no JS actions left");

        let output = doc.to_bytes();
        let content = String::from_utf8_lossy(&output);
        assert!(
            !content.contains("javascript:alert"),
            "javascript: URI should be neutralized"
        );
        assert!(
            !content.contains("app.alert('x')"),
            "JS payload should be removed"
        );
    }

    #[test]
    fn test_diff_pdf_bytes_detects_changes() {
        let elements_old = vec![crate::elements::Element::Paragraph {
            text: "First version".into(),
        }];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let old_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements_old, "Helvetica", 12.0, layout)
                .unwrap();

        let elements_new = vec![
            crate::elements::Element::Paragraph {
                text: "Second version with more content".into(),
            },
            crate::elements::Element::Paragraph {
                text: "Extra paragraph".into(),
            },
        ];
        let new_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements_new, "Helvetica", 12.0, layout)
                .unwrap();

        let diff = diff_pdf_bytes(&old_bytes, &new_bytes).unwrap();

        // Both should have 1 page
        assert_eq!(diff.pages_old, 1, "Old PDF should have 1 page");
        assert_eq!(diff.pages_new, 1, "New PDF should have 1 page");

        // Text should be somewhat similar but not identical
        assert!(
            diff.text_similarity > 0.0 && diff.text_similarity < 1.0,
            "Text similarity should be between 0 and 1 for partially different docs: {}",
            diff.text_similarity
        );

        // There should be some modified objects (content streams differ)
        assert!(
            !diff.modified_objects.is_empty() || !diff.added_objects.is_empty(),
            "Should detect structural changes between different PDFs"
        );
    }

    #[test]
    fn test_diff_pdf_bytes_identical() {
        let elements = vec![crate::elements::Element::Paragraph {
            text: "Same content".into(),
        }];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let diff = diff_pdf_bytes(&bytes, &bytes).unwrap();

        // Identical PDFs should have 100% text similarity
        assert_eq!(
            diff.text_similarity, 1.0,
            "Identical PDFs should have 100% text similarity"
        );

        // No added, removed, or modified objects
        assert!(
            diff.added_objects.is_empty(),
            "Identical PDFs should have no added objects"
        );
        assert!(
            diff.removed_objects.is_empty(),
            "Identical PDFs should have no removed objects"
        );
        assert!(
            diff.modified_objects.is_empty(),
            "Identical PDFs should have no modified objects"
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
