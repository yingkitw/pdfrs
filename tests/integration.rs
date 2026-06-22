use pdfrs::builder::PdfBuilder;
use pdfrs::parallel;
use pdfrs::pdf::PdfDocument;
use std::path::Path;

fn create_test_pdf(path: &str, title: &str) {
    let elements = vec![
        pdfrs::elements::Element::Heading {
            text: title.to_string(),
            level: 1,
        },
        pdfrs::elements::Element::Paragraph {
            text: format!("This is a test document: {}", title),
        },
    ];
    pdfrs::pdf_generator::create_pdf_from_elements_with_layout(
        path,
        &elements,
        "Helvetica",
        12.0,
        pdfrs::pdf_generator::PageLayout::portrait(),
    )
    .unwrap();
}

#[test]
fn test_parallel_merge_pdfs() {
    let pdf1 = "tests/output/int_merge1.pdf";
    let pdf2 = "tests/output/int_merge2.pdf";
    let merged = "tests/output/int_merged.pdf";

    std::fs::create_dir_all("tests/output").unwrap();

    create_test_pdf(pdf1, "Document 1");
    create_test_pdf(pdf2, "Document 2");

    assert!(Path::new(pdf1).exists());
    assert!(Path::new(pdf2).exists());

    let result = parallel::merge_pdfs_parallel(&[pdf1, pdf2], merged);
    assert!(result.is_ok(), "Parallel merge failed: {:?}", result.err());
    assert!(Path::new(merged).exists(), "Merged PDF was not created");

    // Verify the merged PDF is valid and has combined pages
    let doc = PdfDocument::load_from_file(merged).unwrap();
    assert!(!doc.objects.is_empty());

    // Cleanup
    std::fs::remove_file(pdf1).ok();
    std::fs::remove_file(pdf2).ok();
    std::fs::remove_file(merged).ok();
}

#[test]
fn test_builder_api_generates_valid_pdf() {
    let result = PdfBuilder::new()
        .add_heading("Integration Test", 1)
        .add_paragraph("This tests the builder API.")
        .add_code_block("fn main() {}", "rust")
        .add_list_item("Item 1", 0)
        .add_ordered_item(1, "Ordered item", 0)
        .add_horizontal_rule()
        .add_page_break()
        .build_bytes();

    assert!(result.is_ok());
    let bytes = result.unwrap();
    assert!(bytes.len() > 100);
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn test_builder_api_with_layout() {
    let result = PdfBuilder::new()
        .with_layout(pdfrs::pdf_generator::PageLayout::landscape())
        .with_margins(50.0)
        .add_heading("Landscape", 1)
        .build_bytes();

    assert!(result.is_ok());
}

#[test]
fn test_optimized_pdf_generator_compression() {
    let elements = vec![
        pdfrs::elements::Element::Paragraph {
            text: "This is a test document for compression. ".repeat(50),
        },
    ];

    // Web profile (high compression)
    let web_gen = pdfrs::optimization::OptimizedPdfGenerator::new(
        pdfrs::optimization::OptimizationProfile::web(),
    );
    let web_bytes = web_gen.generate_bytes(&elements).unwrap();

    // Print profile (low compression)
    let print_gen = pdfrs::optimization::OptimizedPdfGenerator::new(
        pdfrs::optimization::OptimizationProfile::print(),
    );
    let print_bytes = print_gen.generate_bytes(&elements).unwrap();

    assert!(web_bytes.starts_with(b"%PDF"));
    assert!(print_bytes.starts_with(b"%PDF"));

    // Web should be smaller than print due to higher compression
    assert!(
        web_bytes.len() <= print_bytes.len(),
        "Web-optimized PDF ({}) should not be larger than print ({})",
        web_bytes.len(),
        print_bytes.len()
    );

    // Verify compressed PDFs are valid by parsing them
    let temp_web = "tests/output/int_web.pdf";
    let temp_print = "tests/output/int_print.pdf";
    std::fs::create_dir_all("tests/output").unwrap();
    std::fs::write(temp_web, &web_bytes).unwrap();
    std::fs::write(temp_print, &print_bytes).unwrap();

    let web_doc = pdfrs::pdf::PdfDocument::load_from_file(temp_web).unwrap();
    let print_doc = pdfrs::pdf::PdfDocument::load_from_file(temp_print).unwrap();

    assert!(!web_doc.objects.is_empty());
    assert!(!print_doc.objects.is_empty());

    std::fs::remove_file(temp_web).ok();
    std::fs::remove_file(temp_print).ok();
}

#[test]
fn test_streaming_pdf_generator() {
    let output = "tests/output/int_streaming.pdf";
    std::fs::create_dir_all("tests/output").unwrap();

    let mut pdf_gen = pdfrs::streaming::StreamingPdfGenerator::new(
        output,
        pdfrs::pdf_generator::PageLayout::portrait(),
    )
    .unwrap();

    pdf_gen.add_heading("Streaming Test", 1).unwrap();
    pdf_gen.add_paragraph("This is paragraph one.").unwrap();
    pdf_gen.add_paragraph("This is paragraph two.").unwrap();
    pdf_gen.finish().unwrap();

    assert!(std::path::Path::new(output).exists());

    // Verify it's a valid PDF that can be parsed
    let doc = pdfrs::pdf::PdfDocument::load_from_file(output).unwrap();
    assert!(!doc.objects.is_empty());

    std::fs::remove_file(output).ok();
}

#[test]
fn test_digital_signature_sign_and_verify() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    std::fs::create_dir_all(&out_dir).unwrap();

    let input_pdf = format!("{}/tests/fixtures/simple.pdf", base);
    let signed_pdf = format!("{}/signed_test.pdf", out_dir);

    // Create a simple PDF first if the fixture doesn't exist
    if !std::path::Path::new(&input_pdf).exists() {
        let elements = vec![
            pdfrs::elements::Element::Heading {
                text: "Test Document".to_string(),
                level: 1,
            },
            pdfrs::elements::Element::Paragraph {
                text: "This is a test document for digital signatures.".to_string(),
            },
        ];
        pdfrs::pdf_generator::create_pdf_from_elements_with_layout(
            &input_pdf,
            &elements,
            "Helvetica",
            12.0,
            pdfrs::pdf_generator::PageLayout::portrait(),
        )
        .unwrap();
    }

    // Sign the PDF
    let sig = pdfrs::security::DigitalSignature::new("Test Signer")
        .with_reason("Testing digital signatures")
        .with_location("Test Location");

    pdfrs::pdf_ops::sign_pdf(&input_pdf, &signed_pdf, &sig).unwrap();

    assert!(std::path::Path::new(&signed_pdf).exists());
    assert!(std::fs::metadata(&signed_pdf).unwrap().len() > 0);

    // Verify the signature
    let signatures = pdfrs::pdf_ops::verify_pdf_signature(&signed_pdf).unwrap();
    assert!(!signatures.is_empty(), "Should find at least one signature");

    let sig_info = &signatures[0];
    assert_eq!(sig_info.signer_name, "Test Signer");
    assert_eq!(sig_info.reason.as_deref(), Some("Testing digital signatures"));
    assert_eq!(sig_info.location.as_deref(), Some("Test Location"));

    std::fs::remove_file(&signed_pdf).ok();
}

#[test]
fn test_extract_tables_from_pdf() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    std::fs::create_dir_all(&out_dir).unwrap();

    let table_pdf = format!("{}/table_test.pdf", out_dir);

    // Create a PDF with a simple table using table rows
    let elements = vec![
        pdfrs::elements::Element::TableRow {
            cells: vec!["Name".to_string(), "Age".to_string(), "City".to_string()],
            is_separator: false,
            alignments: vec![
                pdfrs::elements::TableAlignment::Left,
                pdfrs::elements::TableAlignment::Center,
                pdfrs::elements::TableAlignment::Right,
            ],
        },
        pdfrs::elements::Element::TableRow {
            cells: vec!["Alice".to_string(), "30".to_string(), "New York".to_string()],
            is_separator: false,
            alignments: vec![],
        },
        pdfrs::elements::Element::TableRow {
            cells: vec!["Bob".to_string(), "25".to_string(), "London".to_string()],
            is_separator: false,
            alignments: vec![],
        },
    ];

    pdfrs::pdf_generator::create_pdf_from_elements_with_layout(
        &table_pdf,
        &elements,
        "Helvetica",
        12.0,
        pdfrs::pdf_generator::PageLayout::portrait(),
    )
    .unwrap();

    // Extract tables
    let tables = pdfrs::pdf_ops::extract_tables_from_pdf(&table_pdf).unwrap();
    assert!(!tables.is_empty(), "Should find at least one table");

    let csv = &tables[0];
    assert!(csv.contains("Name") || csv.contains("Alice"), "CSV should contain table data");

    std::fs::remove_file(&table_pdf).ok();
}

#[test]
fn test_form_field_detect_and_fill() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    std::fs::create_dir_all(&out_dir).unwrap();

    let form_pdf = format!("{}/form_detect_test.pdf", out_dir);
    let filled_pdf = format!("{}/form_filled_test.pdf", out_dir);

    // Create a PDF with form fields
    let elements = vec![
        pdfrs::elements::Element::Paragraph { text: "Please fill out the form.".to_string() },
    ];

    let form_fields = vec![
        pdfrs::pdf_ops::FormField {
            name: "firstName".to_string(),
            field_type: pdfrs::pdf_ops::FormFieldType::Text,
            x: 100.0,
            y: 700.0,
            width: 200.0,
            height: 20.0,
            default_value: Some("Default".to_string()),
            options: vec![],
            required: true,
        },
        pdfrs::pdf_ops::FormField {
            name: "age".to_string(),
            field_type: pdfrs::pdf_ops::FormFieldType::Text,
            x: 100.0,
            y: 670.0,
            width: 50.0,
            height: 20.0,
            default_value: None,
            options: vec![],
            required: false,
        },
    ];

    pdfrs::pdf_generator::create_pdf_from_elements_with_layout(
        &format!("{}/tmp_form_base.pdf", out_dir),
        &elements,
        "Helvetica",
        12.0,
        pdfrs::pdf_generator::PageLayout::portrait(),
    )
    .unwrap();

    // Use create_pdf_with_form_fields to build a proper form PDF
    pdfrs::pdf_ops::create_pdf_with_form_fields(
        &form_pdf,
        "Please fill out the form.",
        &form_fields,
    )
    .unwrap();

    // Detect fields
    let detected = pdfrs::pdf_ops::detect_form_fields(&form_pdf).unwrap();
    assert!(!detected.is_empty(), "Should detect form fields");

    let first_name = detected.iter().find(|f| f.name == "firstName");
    assert!(first_name.is_some(), "Should find firstName field");
    assert_eq!(first_name.unwrap().field_type, "text");
    assert_eq!(first_name.unwrap().value.as_deref(), Some("Default"));

    // Fill fields
    let mut values = std::collections::HashMap::new();
    values.insert("firstName".to_string(), "Alice".to_string());
    values.insert("age".to_string(), "30".to_string());

    pdfrs::pdf_ops::fill_form_fields(&form_pdf, &filled_pdf, &values).unwrap();

    // Verify filled values
    let filled = pdfrs::pdf_ops::detect_form_fields(&filled_pdf).unwrap();
    let filled_first = filled.iter().find(|f| f.name == "firstName");
    assert!(filled_first.is_some());
    assert_eq!(filled_first.unwrap().value.as_deref(), Some("Alice"));

    let filled_age = filled.iter().find(|f| f.name == "age");
    assert!(filled_age.is_some());
    assert_eq!(filled_age.unwrap().value.as_deref(), Some("30"));

    std::fs::remove_file(&form_pdf).ok();
    std::fs::remove_file(&filled_pdf).ok();
    std::fs::remove_file(&format!("{}/tmp_form_base.pdf", out_dir)).ok();
}

#[test]
fn test_detect_document_structure() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    std::fs::create_dir_all(&out_dir).unwrap();

    let struct_pdf = format!("{}/structure_test.pdf", out_dir);

    // Create a PDF with clear heading hierarchy using markdown
    let markdown = r#"# Introduction

This is the introduction paragraph.

## Background

Some background text here.
More background text.

## Methods

Method description goes here.

# Results

Result paragraph one.
Result paragraph two.
"#;

    let elements = pdfrs::elements::parse_markdown(markdown);
    pdfrs::pdf_generator::create_pdf_from_elements_with_layout(
        &struct_pdf,
        &elements,
        "Helvetica",
        12.0,
        pdfrs::pdf_generator::PageLayout::portrait(),
    )
    .unwrap();

    // Detect structure
    let structure = pdfrs::pdf_ops::detect_document_structure(&struct_pdf).unwrap();

    // Should detect at least some headings (H1 and H2 sizes differ from body)
    assert!(!structure.headings.is_empty(), "Should detect headings in structured PDF");

    // Check that "Introduction" or "Results" is found as a heading
    let has_intro = structure.headings.iter().any(|h| h.text.contains("Introduction"));
    let has_results = structure.headings.iter().any(|h| h.text.contains("Results"));
    assert!(
        has_intro || has_results,
        "Should detect 'Introduction' or 'Results' heading, got: {:?}",
        structure.headings
    );

    // Sections should exist matching headings
    assert!(
        !structure.sections.is_empty(),
        "Should have sections"
    );

    std::fs::remove_file(&struct_pdf).ok();
}
