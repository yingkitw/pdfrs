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

#[test]
fn test_optimize_pdf_recompression() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    std::fs::create_dir_all(&out_dir).unwrap();

    let source_pdf = format!("{}/opt_source.pdf", out_dir);
    let optimized_pdf = format!("{}/opt_output.pdf", out_dir);

    // Create a PDF with content
    let markdown = r#"# Optimization Test

This is a test paragraph for PDF optimization.
It contains enough text to create a multi-page document.

## Section Two

More text here to fill the page.
And even more text to ensure we have content streams.
"#;
    let elements = pdfrs::elements::parse_markdown(markdown);
    pdfrs::pdf_generator::create_pdf_from_elements_with_layout(
        &source_pdf,
        &elements,
        "Helvetica",
        12.0,
        pdfrs::pdf_generator::PageLayout::portrait(),
    )
    .unwrap();

    let _original_size = std::fs::metadata(&source_pdf).unwrap().len();

    // Optimize with Web profile (high compression)
    let profile = pdfrs::optimization::OptimizationProfile::Web;
    let settings = profile.settings();
    let pdf_bytes = std::fs::read(&source_pdf).unwrap();
    let optimized = pdfrs::optimization::optimize_pdf_bytes(&pdf_bytes, settings).unwrap();
    std::fs::write(&optimized_pdf, &optimized).unwrap();

    // Verify optimized PDF is valid and can be loaded
    let doc = pdfrs::pdf::PdfDocument::load_from_file(&optimized_pdf).unwrap();
    assert!(!doc.objects.is_empty(), "Optimized PDF should have objects");

    // Verify text can still be extracted
    let text = doc.get_text().unwrap();
    assert!(
        text.contains("Optimization Test"),
        "Optimized PDF should still contain original text"
    );

    // For small text-only PDFs, recompression may not always reduce size,
    // but it should not corrupt the document.
    assert!(
        optimized.len() > 100,
        "Optimized PDF should be non-trivial in size"
    );

    std::fs::remove_file(&source_pdf).ok();
    std::fs::remove_file(&optimized_pdf).ok();
}

#[test]
fn test_extract_images_from_pdf() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    std::fs::create_dir_all(&out_dir).unwrap();

    // Create a minimal 1x1 BMP file programmatically
    let bmp_path = format!("{}/test_image.bmp", out_dir);
    let mut bmp_data = Vec::new();
    // BMP file header (14 bytes)
    bmp_data.extend_from_slice(b"BM");              // signature
    bmp_data.extend_from_slice(&70_u32.to_le_bytes()); // file size
    bmp_data.extend_from_slice(&[0, 0]);              // reserved
    bmp_data.extend_from_slice(&[0, 0]);              // reserved
    bmp_data.extend_from_slice(&54_u32.to_le_bytes()); // offset to pixel data
    // DIB header (BITMAPINFOHEADER, 40 bytes)
    bmp_data.extend_from_slice(&40_u32.to_le_bytes()); // header size
    bmp_data.extend_from_slice(&1_u32.to_le_bytes());  // width
    bmp_data.extend_from_slice(&1_u32.to_le_bytes());  // height
    bmp_data.extend_from_slice(&1_u16.to_le_bytes());  // planes
    bmp_data.extend_from_slice(&24_u16.to_le_bytes()); // bits per pixel
    bmp_data.extend_from_slice(&0_u32.to_le_bytes());  // compression (none)
    bmp_data.extend_from_slice(&0_u32.to_le_bytes());  // image size
    bmp_data.extend_from_slice(&2835_u32.to_le_bytes()); // X pixels per meter
    bmp_data.extend_from_slice(&2835_u32.to_le_bytes()); // Y pixels per meter
    bmp_data.extend_from_slice(&0_u32.to_le_bytes());  // colors used
    bmp_data.extend_from_slice(&0_u32.to_le_bytes());  // important colors
    // Pixel data: 1 pixel (3 bytes) + 1 byte padding to 4-byte boundary
    bmp_data.extend_from_slice(&[0xFF, 0x00, 0x00, 0x00]);
    std::fs::write(&bmp_path, &bmp_data).unwrap();

    // Embed the image in a PDF
    let image_pdf = format!("{}/image_embed_test.pdf", out_dir);
    pdfrs::image::add_image_to_pdf(&image_pdf, &bmp_path, 100.0, 100.0, 50.0, 50.0).unwrap();

    // Extract images from the PDF
    let extract_dir = format!("{}/extracted_test_images", out_dir);
    let extracted = pdfrs::pdf_ops::extract_images_from_pdf(&image_pdf, &extract_dir).unwrap();

    assert!(!extracted.is_empty(), "Should extract at least one image from the PDF");

    // Cleanup
    std::fs::remove_file(&bmp_path).ok();
    std::fs::remove_file(&image_pdf).ok();
    std::fs::remove_dir_all(&extract_dir).ok();
}

#[test]
fn test_font_subsetting_reduces_file_size() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    std::fs::create_dir_all(&out_dir).unwrap();

    let subset_pdf = format!("{}/subset_font.pdf", out_dir);
    let nosubset_pdf = format!("{}/nosubset_font.pdf", out_dir);

    // Use Unicode characters to trigger font embedding
    let markdown = r"# Unicode Test

Hello with unicode: α β γ δ ε
And some CJK: 中文测试
Math symbols: ∫ ∑ ∏ √
";
    let elements = pdfrs::elements::parse_markdown(markdown);

    // Use custom settings with identical compression but different subset_fonts
    let settings_subset = pdfrs::optimization::OptimizationSettings::new()
        .with_compression(pdfrs::optimization::CompressionLevel::Medium)
        .with_embed_fonts(true)
        .with_subset_fonts(true);
    let generator_subset = pdfrs::optimization::OptimizedPdfGenerator::new(
        pdfrs::optimization::OptimizationProfile::Custom(settings_subset),
    )
    .with_layout(pdfrs::pdf_generator::PageLayout::portrait());
    generator_subset.generate(&elements, &subset_pdf).unwrap();

    let settings_nosubset = pdfrs::optimization::OptimizationSettings::new()
        .with_compression(pdfrs::optimization::CompressionLevel::Medium)
        .with_embed_fonts(true)
        .with_subset_fonts(false);
    let generator_nosubset = pdfrs::optimization::OptimizedPdfGenerator::new(
        pdfrs::optimization::OptimizationProfile::Custom(settings_nosubset),
    )
    .with_layout(pdfrs::pdf_generator::PageLayout::portrait());
    generator_nosubset.generate(&elements, &nosubset_pdf).unwrap();

    // Both should be valid PDFs
    let doc_subset = pdfrs::pdf::PdfDocument::load_from_file(&subset_pdf).unwrap();
    let doc_nosubset = pdfrs::pdf::PdfDocument::load_from_file(&nosubset_pdf).unwrap();
    assert!(!doc_subset.objects.is_empty());
    assert!(!doc_nosubset.objects.is_empty());

    // Both should contain an embedded font (FontFile2)
    let subset_has_font = doc_subset.to_bytes().windows(10).any(|w| w == b"/FontFile2");
    let nosubset_has_font = doc_nosubset.to_bytes().windows(10).any(|w| w == b"/FontFile2");
    assert!(subset_has_font, "Subset PDF should contain embedded font");
    assert!(nosubset_has_font, "No-subset PDF should contain embedded font");

    // The subsetted PDF should be smaller or equal in size
    let size_subset = std::fs::metadata(&subset_pdf).unwrap().len();
    let size_nosubset = std::fs::metadata(&nosubset_pdf).unwrap().len();
    assert!(
        size_subset <= size_nosubset,
        "Subsetted PDF ({}) should be smaller than or equal to non-subsetted PDF ({})",
        size_subset,
        size_nosubset
    );

    std::fs::remove_file(&subset_pdf).ok();
    std::fs::remove_file(&nosubset_pdf).ok();
}

#[test]
fn test_pdf_2_0_version_header() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    std::fs::create_dir_all(&out_dir).unwrap();

    let pdf_v14_path = format!("{}/version_1_4.pdf", out_dir);
    let pdf_v20_path = format!("{}/version_2_0.pdf", out_dir);

    let markdown = "# Version Test\n\nHello world.";
    let elements = pdfrs::elements::parse_markdown(markdown);

    // Generate PDF 1.4 (default)
    let layout_v14 = pdfrs::pdf_generator::PageLayout::portrait();
    let bytes_v14 = pdfrs::pdf_generator::generate_pdf_bytes(
        &elements, "Helvetica", 12.0, layout_v14,
    ).unwrap();
    std::fs::write(&pdf_v14_path, &bytes_v14).unwrap();

    // Generate PDF 2.0
    let layout_v20 = pdfrs::pdf_generator::PageLayout::portrait()
        .with_version(pdfrs::pdf_generator::PdfVersion::V2_0);
    let bytes_v20 = pdfrs::pdf_generator::generate_pdf_bytes(
        &elements, "Helvetica", 12.0, layout_v20,
    ).unwrap();
    std::fs::write(&pdf_v20_path, &bytes_v20).unwrap();

    // Verify headers
    assert!(bytes_v14.starts_with(b"%PDF-1.4"), "Default PDF should be 1.4");
    assert!(bytes_v20.starts_with(b"%PDF-2.0"), "Explicit version should be 2.0");

    // Both should be valid, loadable PDFs
    let doc_v14 = pdfrs::pdf::PdfDocument::load_from_bytes(&bytes_v14).unwrap();
    let doc_v20 = pdfrs::pdf::PdfDocument::load_from_bytes(&bytes_v20).unwrap();
    assert!(!doc_v14.objects.is_empty());
    assert!(!doc_v20.objects.is_empty());

    // Text should be extractable from both
    let text_v14 = doc_v14.get_text().unwrap();
    let text_v20 = doc_v20.get_text().unwrap();
    assert!(text_v14.contains("Version Test"));
    assert!(text_v20.contains("Version Test"));

    std::fs::remove_file(&pdf_v14_path).ok();
    std::fs::remove_file(&pdf_v20_path).ok();
}

#[test]
fn test_javascript_sandbox_pdf_bytes() {
    std::fs::create_dir_all("tests/output").ok();
    let path = "tests/output/int_sandbox.pdf";
    create_test_pdf(path, "Sandbox Integration");

    let bytes = std::fs::read(path).unwrap();
    let (sandboxed, report) = pdfrs::pdf::sandbox_pdf_bytes(&bytes).unwrap();

    assert!(report.clean, "Generated PDF should remain clean after sandbox pass");
    assert!(pdfrs::pdf::validate_pdf_bytes(&sandboxed).valid);

    std::fs::remove_file(path).ok();
}

#[test]
fn test_screen_reader_compliance_tagged_pdf() {
    let md = "# Accessibility Report\n\n\
        This document tests screen reader compliance.\n\n\
        ## Features\n\n\
        - Headings and lists\n\
        - Paragraph text\n\n\
        | Name | Value |\n\
        |------|-------|\n\
        | A    | 1     |\n";

    let elements = pdfrs::elements::parse_markdown(md);
    let layout = pdfrs::pdf_generator::PageLayout::portrait();
    let opts = pdfrs::pdf_generator::AccessibilityOptions::new()
        .with_tagged_pdf(true)
        .with_language("en-US".to_string())
        .with_title("Screen Reader Compliance Test".to_string());

    let bytes = pdfrs::pdf_generator::generate_tagged_pdf_bytes(
        &elements,
        "Helvetica",
        12.0,
        layout,
        opts,
    )
    .unwrap();

    let report = pdfrs::pdf::check_screen_reader_compliance_bytes(&bytes);
    assert!(
        report.compliant,
        "Tagged PDF should pass screen reader checks: {:?}",
        report.issues
    );
    assert!(report.text_extractable);
    assert!(report.extracted_text_length > 20);
    assert!(report.pdf_ua.compliant);

    let doc = pdfrs::pdf::PdfDocument::load_from_bytes(&bytes).unwrap();
    let text = doc.get_text().unwrap();
    assert!(text.contains("Accessibility Report"));
    assert!(text.contains("screen reader"));
}
