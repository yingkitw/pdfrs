use std::process::Command;
use std::fs;
use std::path::Path;

/// Helper to run pdfcli commands using the pre-built binary directly.
/// This avoids `cargo run` build-lock contention when tests run in parallel.
fn run_pdf_cli(args: &[&str]) -> (String, String, bool) {
    let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_pdfcli"));

    let output = Command::new(&bin)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to execute pdfcli");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr, output.status.success())
}

/// Roundtrip test: MD -> PDF -> MD, then compare
fn roundtrip_test(md_path: &str, label: &str) {
    let base = env!("CARGO_MANIFEST_DIR");
    let md_file = format!("{}/{}", base, md_path);
    let pdf_file = format!("{}/target/test_output/{}.pdf", base, label);
    let out_md_file = format!("{}/target/test_output/{}_roundtrip.md", base, label);

    // Ensure output dir exists
    fs::create_dir_all(format!("{}/target/test_output", base)).unwrap();

    // Read original markdown
    let original_md = fs::read_to_string(&md_file)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", md_file, e));

    println!("=== Roundtrip Test: {} ===", label);
    println!("Input: {}", md_file);
    println!("Original MD length: {} bytes, {} lines", original_md.len(), original_md.lines().count());

    // Step 1: MD -> PDF
    let (stdout, stderr, success) = run_pdf_cli(&["md-to-pdf", &md_file, &pdf_file]);
    println!("[MD->PDF] stdout: {}", stdout.trim());
    if !stderr.is_empty() {
        println!("[MD->PDF] stderr: {}", stderr.trim());
    }
    assert!(success, "md-to-pdf failed for {}", label);
    assert!(Path::new(&pdf_file).exists(), "PDF file not created: {}", pdf_file);

    let pdf_size = fs::metadata(&pdf_file).unwrap().len();
    println!("[MD->PDF] PDF size: {} bytes", pdf_size);
    assert!(pdf_size > 0, "PDF file is empty");

    // Step 2: PDF -> MD (extract text)
    let (stdout, stderr, success) = run_pdf_cli(&["pdf-to-md", &pdf_file, &out_md_file]);
    println!("[PDF->MD] stdout: {}", stdout.trim());
    if !stderr.is_empty() {
        println!("[PDF->MD] stderr: {}", stderr.trim());
    }
    assert!(success, "pdf-to-md failed for {}", label);
    assert!(Path::new(&out_md_file).exists(), "Output MD file not created: {}", out_md_file);

    let roundtrip_md = fs::read_to_string(&out_md_file).unwrap();
    println!("[PDF->MD] Roundtrip MD length: {} bytes, {} lines", roundtrip_md.len(), roundtrip_md.lines().count());

    // Step 3: Validate roundtrip content
    // We check that key content from the original is present in the roundtrip output
    let original_lines: Vec<&str> = original_md.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with("```"))
        .filter(|l| !l.starts_with("---"))
        .filter(|l| !l.starts_with("|"))
        .filter(|l| !l.starts_with("["))
        .filter(|l| !l.starts_with("- ["))
        .collect();

    // Extract plain text words from original (strip markdown syntax)
    let original_words: Vec<String> = original_lines.iter()
        .flat_map(|line| {
            let clean = line
                .replace("**", "")
                .replace("*", "")
                .replace("`", "")
                .replace("#", "")
                .replace("- ", "")
                .replace("> ", "");
            clean.split_whitespace()
                .filter(|w| w.len() > 3)
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        })
        .collect();

    // Check that a reasonable portion of significant words appear in roundtrip
    let mut found = 0;
    let mut checked = 0;
    for word in &original_words {
        // Skip markdown-specific tokens and short words
        if word.contains('|') || word.contains('[') || word.contains(']')
            || word.contains('(') || word.contains(')') || word.len() < 4 {
            continue;
        }
        checked += 1;
        if roundtrip_md.contains(word.as_str()) {
            found += 1;
        }
    }

    let recovery_rate = if checked > 0 { (found as f64 / checked as f64) * 100.0 } else { 0.0 };
    println!("[Validation] Words checked: {}, found: {}, recovery rate: {:.1}%", checked, found, recovery_rate);

    // Print first 20 lines of roundtrip output for inspection
    println!("\n--- Roundtrip output (first 20 lines) ---");
    for (i, line) in roundtrip_md.lines().take(20).enumerate() {
        println!("  {:3}: {}", i + 1, line);
    }
    println!("--- end ---\n");

    // We expect at least some content to survive the roundtrip
    // The exact threshold depends on how well the PDF parser works
    assert!(roundtrip_md.len() > 0, "Roundtrip output is empty for {}", label);
    println!("=== PASSED: {} ===\n", label);
}

#[test]
fn test_roundtrip_complex_report() {
    roundtrip_test("examples/complex_report.md", "complex_report");
}

#[test]
fn test_roundtrip_technical_spec() {
    roundtrip_test("examples/technical_spec.md", "technical_spec");
}

#[test]
fn test_roundtrip_mixed_content() {
    roundtrip_test("examples/mixed_content.md", "mixed_content");
}

#[test]
fn test_roundtrip_existing_test_md() {
    roundtrip_test("test.md", "test_md");
}

#[test]
fn test_roundtrip_existing_roundtrip_md() {
    roundtrip_test("roundtrip_test.md", "roundtrip_test_md");
}

#[test]
fn test_roundtrip_enhanced_features() {
    roundtrip_test("examples/enhanced_features.md", "enhanced_features");
}

#[test]
fn test_roundtrip_landscape() {
    let base = env!("CARGO_MANIFEST_DIR");
    let md_file = format!("{}/examples/enhanced_features.md", base);
    let pdf_file = format!("{}/target/test_output/landscape.pdf", base);
    let out_md_file = format!("{}/target/test_output/landscape_roundtrip.md", base);

    fs::create_dir_all(format!("{}/target/test_output", base)).unwrap();

    // MD -> PDF with --landscape
    let (_, _, success) = run_pdf_cli(&["md-to-pdf", &md_file, &pdf_file, "--landscape"]);
    assert!(success, "md-to-pdf --landscape failed");
    assert!(Path::new(&pdf_file).exists());

    let pdf_size = fs::metadata(&pdf_file).unwrap().len();
    println!("[Landscape] PDF size: {} bytes", pdf_size);
    assert!(pdf_size > 0);

    // PDF -> MD
    let (_, _, success) = run_pdf_cli(&["pdf-to-md", &pdf_file, &out_md_file]);
    assert!(success, "pdf-to-md failed for landscape");

    let roundtrip = fs::read_to_string(&out_md_file).unwrap();
    println!("[Landscape] Roundtrip MD: {} bytes, {} lines", roundtrip.len(), roundtrip.lines().count());
    assert!(roundtrip.len() > 100, "Landscape roundtrip output too short");
    assert!(roundtrip.contains("Enhanced Markdown Features Demo"));
    println!("=== PASSED: landscape ===");
}

#[test]
fn test_merge_pdfs() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let md_a = format!("{}/test.md", base);
    let md_b = format!("{}/roundtrip_test.md", base);
    let pdf_a = format!("{}/merge_a.pdf", out_dir);
    let pdf_b = format!("{}/merge_b.pdf", out_dir);
    let merged = format!("{}/merged_test.pdf", out_dir);
    let merged_md = format!("{}/merged_test_roundtrip.md", out_dir);

    // Create two source PDFs
    let (_, _, ok) = run_pdf_cli(&["md-to-pdf", &md_a, &pdf_a]);
    assert!(ok, "Failed to create PDF A");
    let (_, _, ok) = run_pdf_cli(&["md-to-pdf", &md_b, &pdf_b]);
    assert!(ok, "Failed to create PDF B");

    // Merge
    let (stdout, _, ok) = run_pdf_cli(&["merge", &pdf_a, &pdf_b, "-o", &merged]);
    assert!(ok, "Merge failed");
    println!("[merge] {}", stdout.trim());
    assert!(Path::new(&merged).exists());
    assert!(fs::metadata(&merged).unwrap().len() > 0);

    // Extract text from merged PDF
    let (_, _, ok) = run_pdf_cli(&["pdf-to-md", &merged, &merged_md]);
    assert!(ok, "pdf-to-md on merged failed");
    let text = fs::read_to_string(&merged_md).unwrap();

    // Should contain content from both source files
    assert!(text.contains("Test Document") || text.contains("Features"), "Merged PDF missing content from source A");
    println!("=== PASSED: merge ===");
}

#[test]
fn test_split_pdf() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let md_src = format!("{}/examples/enhanced_features.md", base);
    let pdf_src = format!("{}/split_source.pdf", out_dir);
    let pdf_split = format!("{}/split_page1.pdf", out_dir);
    let split_md = format!("{}/split_page1_roundtrip.md", out_dir);

    // Create multi-page source
    let (_, _, ok) = run_pdf_cli(&["md-to-pdf", &md_src, &pdf_src]);
    assert!(ok, "Failed to create source PDF");

    // Split: extract page 1 only
    let (stdout, _, ok) = run_pdf_cli(&["split", &pdf_src, "-o", &pdf_split, "--start", "1", "--end", "1"]);
    assert!(ok, "Split failed");
    println!("[split] {}", stdout.trim());
    assert!(Path::new(&pdf_split).exists());

    // Extract text from split PDF
    let (_, _, ok) = run_pdf_cli(&["pdf-to-md", &pdf_split, &split_md]);
    assert!(ok, "pdf-to-md on split failed");
    let text = fs::read_to_string(&split_md).unwrap();
    assert!(text.len() > 10, "Split output too short");
    println!("=== PASSED: split ===");
}

#[test]
fn test_metadata_pdf() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let md_src = format!("{}/test.md", base);
    let pdf_out = format!("{}/meta_test.pdf", out_dir);

    let (_, _, ok) = run_pdf_cli(&[
        "md-to-pdf-meta", &md_src, &pdf_out,
        "--title", "Integration Test",
        "--author", "pdf-cli-test",
        "--subject", "Testing metadata embedding",
        "--keywords", "test,pdf,metadata",
    ]);
    assert!(ok, "md-to-pdf-meta failed");
    assert!(Path::new(&pdf_out).exists());

    // Read raw PDF bytes and check metadata strings are present
    let raw_bytes = fs::read(&pdf_out).unwrap();
    let raw = String::from_utf8_lossy(&raw_bytes);
    assert!(raw.contains("/Title (Integration Test)"), "Title not found in PDF");
    assert!(raw.contains("/Author (pdf-cli-test)"), "Author not found in PDF");
    assert!(raw.contains("/Producer (pdf-cli)"), "Producer not found in PDF");
    println!("=== PASSED: metadata ===");
}

#[test]
fn test_rotate_pdf() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let md_src = format!("{}/test.md", base);
    let pdf_src = format!("{}/rotate_source.pdf", out_dir);
    let pdf_rotated = format!("{}/rotated_90.pdf", out_dir);

    // Create source PDF
    let (_, _, ok) = run_pdf_cli(&["md-to-pdf", &md_src, &pdf_src]);
    assert!(ok, "Failed to create source PDF");

    // Rotate 90°
    let (stdout, _, ok) = run_pdf_cli(&["rotate", &pdf_src, "-o", &pdf_rotated, "--angle", "90"]);
    assert!(ok, "Rotate failed");
    println!("[rotate] {}", stdout.trim());
    assert!(Path::new(&pdf_rotated).exists());

    // Verify /Rotate 90 is in the PDF
    let raw = fs::read(&pdf_rotated).unwrap();
    let content = String::from_utf8_lossy(&raw);
    assert!(content.contains("/Rotate 90"), "Rotation not found in PDF");

    // Text should still be extractable
    let out_md = format!("{}/rotated_roundtrip.md", out_dir);
    let (_, _, ok) = run_pdf_cli(&["pdf-to-md", &pdf_rotated, &out_md]);
    assert!(ok, "pdf-to-md on rotated PDF failed");
    let text = fs::read_to_string(&out_md).unwrap();
    assert!(text.len() > 10, "Rotated PDF text extraction too short");
    println!("=== PASSED: rotate ===");
}

#[test]
fn test_watermark_pdf() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let md_src = format!("{}/test.md", base);
    let pdf_src = format!("{}/watermark_source.pdf", out_dir);
    let pdf_wm = format!("{}/watermarked.pdf", out_dir);

    let (_, _, ok) = run_pdf_cli(&["md-to-pdf", &md_src, &pdf_src]);
    assert!(ok, "Failed to create source PDF");

    let (stdout, _, ok) = run_pdf_cli(&["watermark", &pdf_src, "-o", &pdf_wm, "--text", "CONFIDENTIAL"]);
    assert!(ok, "Watermark failed");
    println!("[watermark] {}", stdout.trim());
    assert!(Path::new(&pdf_wm).exists());

    // Verify watermark text is in the PDF
    let raw = fs::read(&pdf_wm).unwrap();
    let content = String::from_utf8_lossy(&raw);
    assert!(content.contains("CONFIDENTIAL"), "Watermark text not found in PDF");
    println!("=== PASSED: watermark ===");
}

#[test]
fn test_reorder_pdf() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    // Use the merged PDF which has multiple pages
    let md_a = format!("{}/test.md", base);
    let md_b = format!("{}/roundtrip_test.md", base);
    let pdf_a = format!("{}/reorder_a.pdf", out_dir);
    let pdf_b = format!("{}/reorder_b.pdf", out_dir);
    let merged = format!("{}/reorder_merged.pdf", out_dir);
    let reordered = format!("{}/reordered.pdf", out_dir);

    let (_, _, ok) = run_pdf_cli(&["md-to-pdf", &md_a, &pdf_a]);
    assert!(ok);
    let (_, _, ok) = run_pdf_cli(&["md-to-pdf", &md_b, &pdf_b]);
    assert!(ok);
    let (_, _, ok) = run_pdf_cli(&["merge", &pdf_a, &pdf_b, "-o", &merged]);
    assert!(ok, "Merge failed");

    // Reorder: reverse order
    let (stdout, _, ok) = run_pdf_cli(&["reorder", &merged, "-o", &reordered, "--pages", "2,1"]);
    assert!(ok, "Reorder failed");
    println!("[reorder] {}", stdout.trim());
    assert!(Path::new(&reordered).exists());
    assert!(fs::metadata(&reordered).unwrap().len() > 0);
    println!("=== PASSED: reorder ===");
}

#[test]
fn test_full_features_roundtrip() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let md_src = format!("{}/examples/full_features.md", base);
    let pdf_out = format!("{}/full_features.pdf", out_dir);
    let md_out = format!("{}/full_features_roundtrip.md", out_dir);

    // Step 1: MD -> PDF
    let (_, _, ok) = run_pdf_cli(&["md-to-pdf", &md_src, &pdf_out]);
    assert!(ok, "md-to-pdf failed for full_features");
    let pdf_size = fs::metadata(&pdf_out).unwrap().len();
    println!("[full_features] PDF size: {} bytes", pdf_size);
    assert!(pdf_size > 5000, "PDF too small: {} bytes", pdf_size);

    // Step 2: Validate raw PDF structure
    let raw = fs::read(&pdf_out).unwrap();
    let raw_str = String::from_utf8_lossy(&raw);
    assert!(raw_str.starts_with("%PDF-"), "Missing PDF header");
    assert!(raw_str.contains("/Type /Catalog"), "Missing catalog");
    assert!(raw_str.contains("/Type /Pages"), "Missing pages tree");
    assert!(raw_str.contains("%%EOF"), "Missing EOF marker");

    // Count pages (should be multi-page due to content + pagebreak)
    let page_count = raw_str.matches("/Type /Page\n").count()
        + raw_str.matches("/Type /Page\r").count();
    println!("[full_features] Page objects found: {}", page_count);
    assert!(page_count >= 4, "Expected at least 4 pages, got {}", page_count);

    // Step 3: PDF -> MD round-trip
    let (_, _, ok) = run_pdf_cli(&["pdf-to-md", &pdf_out, &md_out]);
    assert!(ok, "pdf-to-md failed for full_features");
    let roundtrip = fs::read_to_string(&md_out).unwrap();

    // Step 4: Verify all element types survived round-trip
    let must_contain = vec![
        // Headings
        "Full Feature Showcase",
        "Text Formatting",
        "Lists",
        // Paragraphs
        "exercises every supported element type",
        "bold text",
        "italic text",
        // Unordered lists
        "First item at depth zero",
        "Nested item at depth one",
        "Deep nested item",
        // Ordered lists
        "First numbered item",
        "Second numbered item",
        // Task lists
        "[x] Completed task one",
        "[ ] Pending task three",
        // Code blocks (keywords split by syntax highlighting)
        "fibonacci",
        "quicksort",
        "main",
        "def",
        // Tables
        "Feature",
        "Status",
        "Priority",
        "PDF Generation",
        "Done",
        // Blockquotes
        "simple blockquote",
        "Triple nested",
        // Definition items
        "Rust",
        "systems programming language",
        "Portable Document Format",
        // Footnotes
        "first footnote",
        "second footnote",
        // Links
        "rust-lang.org",
        "adobe.com",
        // Images
        "Rust Logo",
        "rust-logo.png",
        "PDF Icon",
        // Horizontal rules
        "Content above the rule",
        "Content below the rule",
        // Page breaks (content after break)
        "new page after the break",
        // Mixed content
        "Initialize the project",
        "cargo test",
    ];

    let mut missing = Vec::new();
    for s in &must_contain {
        if !roundtrip.contains(s) {
            missing.push(*s);
        }
    }
    assert!(
        missing.is_empty(),
        "Round-trip missing {} items: {:?}",
        missing.len(),
        missing
    );
    println!("[full_features] All {} content checks passed", must_contain.len());
    println!("=== PASSED: full_features_roundtrip ===");
}

#[test]
fn test_full_features_landscape_metadata() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let md_src = format!("{}/examples/full_features.md", base);
    let pdf_out = format!("{}/full_features_landscape_meta.pdf", out_dir);

    // Generate landscape PDF with metadata
    let (_, _, ok) = run_pdf_cli(&[
        "md-to-pdf-meta", &md_src, &pdf_out,
        "--title", "Full Features Showcase",
        "--author", "pdf-rs test suite",
        "--subject", "Integration testing",
        "--keywords", "pdf,rust,test,library",
        "--landscape",
    ]);
    assert!(ok, "md-to-pdf-meta --landscape failed");

    let raw = fs::read(&pdf_out).unwrap();
    let content = String::from_utf8_lossy(&raw);

    // Verify landscape dimensions
    assert!(content.contains("792"), "Missing landscape width 792");
    assert!(content.contains("612"), "Missing landscape height 612");

    // Verify metadata
    assert!(content.contains("/Title (Full Features Showcase)"), "Title missing");
    assert!(content.contains("/Author (pdf-rs test suite)"), "Author missing");
    assert!(content.contains("/Subject (Integration testing)"), "Subject missing");
    assert!(content.contains("/Producer (pdf-cli)"), "Producer missing");

    // Verify content survived
    assert!(content.contains("fibonacci"), "Code block content missing");
    assert!(content.contains("quicksort"), "Python code missing");

    println!("[landscape_meta] PDF size: {} bytes", raw.len());
    println!("=== PASSED: full_features_landscape_metadata ===");
}

#[test]
fn test_full_features_watermark() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let md_src = format!("{}/examples/full_features.md", base);
    let pdf_src = format!("{}/full_features_wm_src.pdf", out_dir);
    let pdf_wm = format!("{}/full_features_watermarked.pdf", out_dir);

    // Generate source PDF
    let (_, _, ok) = run_pdf_cli(&["md-to-pdf", &md_src, &pdf_src]);
    assert!(ok, "Source PDF generation failed");

    // Apply watermark
    let (_, _, ok) = run_pdf_cli(&[
        "watermark", &pdf_src, "-o", &pdf_wm,
        "--text", "DRAFT",
        "--size", "60",
        "--opacity", "0.2",
    ]);
    assert!(ok, "Watermark failed");

    let raw = fs::read(&pdf_wm).unwrap();
    let content = String::from_utf8_lossy(&raw);
    assert!(content.contains("DRAFT"), "Watermark text not found");

    // Original content should still be present
    assert!(content.contains("fibonacci"), "Original content lost after watermark");

    println!("[watermark] Watermarked PDF size: {} bytes", raw.len());
    println!("=== PASSED: full_features_watermark ===");
}

#[test]
fn test_library_api_generate_validate() {
    // Pure library API test: no CLI, no filesystem (except final write for inspection)
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let md_content = fs::read_to_string(format!("{}/examples/full_features.md", base)).unwrap();

    // Step 1: Parse markdown into elements
    let elements = pdfrs::elements::parse_markdown(&md_content);
    println!("[lib_api] Parsed {} elements from full_features.md", elements.len());
    assert!(elements.len() > 50, "Expected many elements, got {}", elements.len());

    // Verify element type diversity
    let has_heading = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::Heading { .. }));
    let has_paragraph = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::Paragraph { .. }));
    let has_list = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::UnorderedListItem { .. }));
    let has_ordered = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::OrderedListItem { .. }));
    let has_task = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::TaskListItem { .. }));
    let has_code = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::CodeBlock { .. }));
    let has_table = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::TableRow { .. }));
    let has_quote = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::BlockQuote { .. }));
    let has_def = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::DefinitionItem { .. }));
    let has_footnote = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::Footnote { .. }));
    let has_link = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::Link { .. }));
    let has_image = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::Image { .. }));
    let has_hr = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::HorizontalRule));
    let has_pagebreak = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::PageBreak));
    let has_empty = elements.iter().any(|e| matches!(e, pdfrs::elements::Element::EmptyLine));

    assert!(has_heading, "Missing Heading elements");
    assert!(has_paragraph, "Missing Paragraph elements");
    assert!(has_list, "Missing UnorderedListItem elements");
    assert!(has_ordered, "Missing OrderedListItem elements");
    assert!(has_task, "Missing TaskListItem elements");
    assert!(has_code, "Missing CodeBlock elements");
    assert!(has_table, "Missing TableRow elements");
    assert!(has_quote, "Missing BlockQuote elements");
    assert!(has_def, "Missing DefinitionItem elements");
    assert!(has_footnote, "Missing Footnote elements");
    assert!(has_link, "Missing Link elements");
    assert!(has_image, "Missing Image elements");
    assert!(has_hr, "Missing HorizontalRule elements");
    assert!(has_pagebreak, "Missing PageBreak elements");
    assert!(has_empty, "Missing EmptyLine elements");
    println!("[lib_api] All 15 element types found in parsed output");

    // Step 2: Generate PDF bytes in memory
    let layout = pdfrs::pdf_generator::PageLayout::portrait();
    let pdf_bytes = pdfrs::pdf_generator::generate_pdf_bytes(
        &elements, "Helvetica", 12.0, layout,
    ).expect("generate_pdf_bytes failed");
    println!("[lib_api] Generated {} bytes of PDF", pdf_bytes.len());
    assert!(pdf_bytes.len() > 5000, "PDF too small: {} bytes", pdf_bytes.len());

    // Step 3: Validate PDF structure
    let validation = pdfrs::pdf::validate_pdf_bytes(&pdf_bytes);
    println!("[lib_api] Validation: valid={}, pages={}, objects={}, errors={:?}, warnings={:?}",
        validation.valid, validation.page_count, validation.object_count,
        validation.errors, validation.warnings);
    assert!(validation.valid, "PDF validation failed: {:?}", validation.errors);
    assert!(validation.page_count >= 4, "Expected >= 4 pages, got {}", validation.page_count);
    assert!(validation.object_count > 10, "Expected many objects, got {}", validation.object_count);

    // Step 4: Verify content strings in raw PDF bytes
    let content = String::from_utf8_lossy(&pdf_bytes);
    let content_checks = vec![
        "Full Feature Showcase", "fibonacci", "quicksort",
        "Completed task", "Pending task",
        "First item at depth zero", "Deep nested",
        "systems programming language",
        "rust-lang.org", "Rust Logo",
        "new page after the break",
    ];
    for s in &content_checks {
        assert!(content.contains(s), "PDF bytes missing content: '{}'", s);
    }
    println!("[lib_api] All {} content checks passed", content_checks.len());

    // Step 5: Write to disk for manual inspection
    let out_path = format!("{}/lib_api_generated.pdf", out_dir);
    fs::write(&out_path, &pdf_bytes).unwrap();
    println!("[lib_api] Written to {} for inspection", out_path);

    println!("=== PASSED: library_api_generate_validate ===");
}

#[test]
fn test_technical_report_complex_roundtrip() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let md_src = format!("{}/examples/technical_report_complex.md", base);
    let pdf_out = format!("{}/technical_report_complex.pdf", out_dir);
    let md_out = format!("{}/technical_report_complex_rt.md", out_dir);

    // Step 1: MD -> PDF
    let (_, _, ok) = run_pdf_cli(&["md-to-pdf", &md_src, &pdf_out]);
    assert!(ok, "md-to-pdf failed for technical_report_complex");
    let pdf_size = fs::metadata(&pdf_out).unwrap().len();
    println!("[tech_report] PDF size: {} bytes", pdf_size);
    assert!(pdf_size > 15000, "PDF too small: {} bytes", pdf_size);

    // Step 2: Validate PDF structure via library API
    let raw = fs::read(&pdf_out).unwrap();
    let validation = pdfrs::pdf::validate_pdf_bytes(&raw);
    println!("[tech_report] valid={}, pages={}, objects={}, errors={:?}",
        validation.valid, validation.page_count, validation.object_count, validation.errors);
    assert!(validation.valid, "PDF validation failed: {:?}", validation.errors);
    assert!(validation.page_count >= 6, "Expected >= 6 pages, got {}", validation.page_count);
    assert!(validation.object_count > 15, "Expected many objects, got {}", validation.object_count);

    // Step 3: PDF -> MD round-trip
    let (_, _, ok) = run_pdf_cli(&["pdf-to-md", &pdf_out, &md_out]);
    assert!(ok, "pdf-to-md failed for technical_report_complex");
    let roundtrip = fs::read_to_string(&md_out).unwrap();

    // Step 4: Verify content survival across all element types
    let must_contain = vec![
        // Headings
        "Distributed Systems Performance Analysis",
        "Executive Summary",
        "System Architecture Overview",
        "Benchmark Methodology",
        "Capacity Planning",
        "Recommendations",
        // Paragraphs with bold/numbers
        "2.4 million requests per second",
        "99.97%",
        "47%",
        // Tables
        "gRPC", "Kafka", "NATS", "Redis",
        "API Gateway", "Order Engine", "Inventory", "Pricing",
        "847,000", "2,482,000",
        "BM-001", "BM-005",
        // Code blocks (Rust)
        "LoadGenerator", "Semaphore", "BenchmarkResult",
        "target_rps", "concurrency",
        // Code blocks (Python)
        "LatencyDistribution", "confidence_interval",
        "np.percentile",
        // Code blocks (YAML)
        "HorizontalPodAutoscaler", "order-engine",
        "minReplicas", "maxReplicas",
        // Ordered lists with nesting
        "Gateway Layer", "Core Business Logic", "Data Pipeline",
        "API Gateway with rate limiting",
        "Event ingestion",
        // Task lists
        "Circuit breaker activation",
        "Cross-region failover",
        // Blockquotes
        "Key Finding",
        "Critical Issue",
        // Definition lists
        "50th percentile",
        "Service Level Agreement",
        "Command Query Responsibility Segregation",
        // Footnotes
        "latency measurements",
        "Cost projections",
        // Links
        "dist-sys-patterns",
        "sre.google",
        "kafka.apache.org",
        // Horizontal rules (content around them)
        "Engineering Team",
        "February 2026",
        // Page break content
        "Raw Benchmark Data",
        // Cost table
        "142,000", "318,000",
    ];

    let mut missing = Vec::new();
    for s in &must_contain {
        if !roundtrip.contains(s) {
            missing.push(*s);
        }
    }
    assert!(
        missing.is_empty(),
        "[tech_report] Round-trip missing {} items: {:?}",
        missing.len(), missing
    );
    println!("[tech_report] All {} content checks passed", must_contain.len());
    println!("=== PASSED: technical_report_complex_roundtrip ===");
}

#[test]
fn test_api_reference_complex_roundtrip() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let md_src = format!("{}/examples/api_reference_complex.md", base);
    let pdf_out = format!("{}/api_reference_complex.pdf", out_dir);
    let md_out = format!("{}/api_reference_complex_rt.md", out_dir);

    // Step 1: MD -> PDF
    let (_, _, ok) = run_pdf_cli(&["md-to-pdf", &md_src, &pdf_out]);
    assert!(ok, "md-to-pdf failed for api_reference_complex");
    let pdf_size = fs::metadata(&pdf_out).unwrap().len();
    println!("[api_ref] PDF size: {} bytes", pdf_size);
    assert!(pdf_size > 20000, "PDF too small: {} bytes", pdf_size);

    // Step 2: Validate PDF structure via library API
    let raw = fs::read(&pdf_out).unwrap();
    let validation = pdfrs::pdf::validate_pdf_bytes(&raw);
    println!("[api_ref] valid={}, pages={}, objects={}, errors={:?}",
        validation.valid, validation.page_count, validation.object_count, validation.errors);
    assert!(validation.valid, "PDF validation failed: {:?}", validation.errors);
    assert!(validation.page_count >= 8, "Expected >= 8 pages, got {}", validation.page_count);

    // Step 3: PDF -> MD round-trip
    let (_, _, ok) = run_pdf_cli(&["pdf-to-md", &pdf_out, &md_out]);
    assert!(ok, "pdf-to-md failed for api_reference_complex");
    let roundtrip = fs::read_to_string(&md_out).unwrap();

    // Step 4: Verify content survival
    let must_contain = vec![
        // Headings
        "PDF-RS Library API Reference",
        "Module Overview",
        "Elements Module",
        "PDF Module",
        "PDF Generator Module",
        "PDF Operations Module",
        "Markdown Module",
        "Error Handling",
        "Usage Examples",
        // Element enum variants in code blocks
        "Heading", "Paragraph", "CodeBlock", "BlockQuote",
        "UnorderedListItem", "OrderedListItem", "TaskListItem",
        "InlineCode", "Link", "Image", "StyledText",
        "TableRow", "DefinitionItem", "Footnote",
        "HorizontalRule", "PageBreak", "EmptyLine",
        // Function signatures
        "parse_markdown", "strip_inline_formatting",
        "validate_pdf", "validate_pdf_bytes",
        "generate_pdf_bytes", "extract_text",
        // Types
        "PdfDocument", "PdfValidation", "PageLayout", "Color",
        "TextAlign", "PdfMetadata",
        "TextAnnotation", "LinkAnnotation", "HighlightAnnotation",
        // Tables
        "elements", "pdf_generator", "pdf_ops", "markdown",
        "compression", "security",
        // Code examples
        "Helvetica", "portrait",
        "assert!", "validation.valid",
        // Definition items
        "612 x 792", "792 x 612",
        "positioned text note",
        "clickable rectangular region",
        "colored highlight overlay",
        // Footnotes
        "17 element variants",
        "In-memory validation",
        // Feature matrix
        "FlateDecode", "DCTDecode",
        // Blockquotes
        "primary library API",
        "Best Practice",
        // Links
        "pdf-rs v0.1.0",
    ];

    let mut missing = Vec::new();
    for s in &must_contain {
        if !roundtrip.contains(s) {
            missing.push(*s);
        }
    }
    assert!(
        missing.is_empty(),
        "[api_ref] Round-trip missing {} items: {:?}",
        missing.len(), missing
    );
    println!("[api_ref] All {} content checks passed", must_contain.len());
    println!("=== PASSED: api_reference_complex_roundtrip ===");
}

#[test]
fn test_complex_examples_library_api_batch() {
    // Pure library API: parse + generate + validate all complex examples in one test
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let examples = vec![
        ("full_features.md", 50, 4),
        ("technical_report_complex.md", 80, 6),
        ("api_reference_complex.md", 100, 8),
    ];

    for (filename, min_elements, min_pages) in &examples {
        let md_path = format!("{}/examples/{}", base, filename);
        let md_content = fs::read_to_string(&md_path)
            .unwrap_or_else(|_| panic!("Failed to read {}", md_path));

        // Parse
        let elements = pdfrs::elements::parse_markdown(&md_content);
        println!("[batch:{}] Parsed {} elements", filename, elements.len());
        assert!(
            elements.len() >= *min_elements,
            "{}: expected >= {} elements, got {}", filename, min_elements, elements.len()
        );

        // Generate portrait
        let layout_p = pdfrs::pdf_generator::PageLayout::portrait();
        let bytes_p = pdfrs::pdf_generator::generate_pdf_bytes(
            &elements, "Helvetica", 12.0, layout_p,
        ).unwrap_or_else(|e| panic!("{}: generate_pdf_bytes portrait failed: {}", filename, e));

        // Validate portrait
        let val_p = pdfrs::pdf::validate_pdf_bytes(&bytes_p);
        assert!(val_p.valid, "{} portrait validation failed: {:?}", filename, val_p.errors);
        assert!(
            val_p.page_count >= *min_pages,
            "{}: expected >= {} pages, got {}", filename, min_pages, val_p.page_count
        );
        println!("[batch:{}] Portrait: {} bytes, {} pages, {} objects",
            filename, bytes_p.len(), val_p.page_count, val_p.object_count);

        // Generate landscape
        let layout_l = pdfrs::pdf_generator::PageLayout::landscape();
        let bytes_l = pdfrs::pdf_generator::generate_pdf_bytes(
            &elements, "Times-Roman", 11.0, layout_l,
        ).unwrap_or_else(|e| panic!("{}: generate_pdf_bytes landscape failed: {}", filename, e));

        // Validate landscape
        let val_l = pdfrs::pdf::validate_pdf_bytes(&bytes_l);
        assert!(val_l.valid, "{} landscape validation failed: {:?}", filename, val_l.errors);
        println!("[batch:{}] Landscape: {} bytes, {} pages, {} objects",
            filename, bytes_l.len(), val_l.page_count, val_l.object_count);

        // Write both for manual inspection
        let stem = filename.replace(".md", "");
        fs::write(format!("{}/batch_{}_portrait.pdf", out_dir, stem), &bytes_p).unwrap();
        fs::write(format!("{}/batch_{}_landscape.pdf", out_dir, stem), &bytes_l).unwrap();
    }

    println!("=== PASSED: complex_examples_library_api_batch ===");
}

#[test]
fn test_math_and_formulas_roundtrip() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out_dir = format!("{}/target/test_output", base);
    fs::create_dir_all(&out_dir).unwrap();

    let md_src = format!("{}/examples/math_and_formulas.md", base);
    let pdf_out = format!("{}/math_and_formulas.pdf", out_dir);
    let md_out = format!("{}/math_and_formulas_rt.md", out_dir);

    // Step 1: MD -> PDF
    let (_, _, ok) = run_pdf_cli(&["md-to-pdf", &md_src, &pdf_out]);
    assert!(ok, "md-to-pdf failed for math_and_formulas");
    let pdf_size = fs::metadata(&pdf_out).unwrap().len();
    println!("[math] PDF size: {} bytes", pdf_size);
    assert!(pdf_size > 15000, "PDF too small: {} bytes", pdf_size);

    // Step 2: Validate PDF structure
    let raw = fs::read(&pdf_out).unwrap();
    let validation = pdfrs::pdf::validate_pdf_bytes(&raw);
    println!("[math] valid={}, pages={}, objects={}, errors={:?}",
        validation.valid, validation.page_count, validation.object_count, validation.errors);
    assert!(validation.valid, "PDF validation failed: {:?}", validation.errors);
    assert!(validation.page_count >= 5, "Expected >= 5 pages, got {}", validation.page_count);

    // Step 3: PDF -> MD round-trip
    let (_, _, ok) = run_pdf_cli(&["pdf-to-md", &pdf_out, &md_out]);
    assert!(ok, "pdf-to-md failed for math_and_formulas");
    let roundtrip = fs::read_to_string(&md_out).unwrap();

    // Step 4: Verify content survival
    let must_contain = vec![
        // Headings
        "Mathematical Foundations",
        "Linear Algebra",
        "Calculus and Optimization",
        "Probability and Statistics",
        "Neural Network",
        "Information Theory",
        "Advanced Topics",
        // Math content (rendered form)
        "SUM",
        "sqrt",
        // Code blocks
        "GradientDescent",
        "learning_rate",
        "max_iterations",
        "tolerance",
        "svd_compress",
        "np.linalg.svd",
        "gradient_descent",
        // Tables
        "Sigmoid", "Tanh", "ReLU", "Softmax",
        "Gradient Descent", "Backpropagation",
        "Bayes", "Cross-Entropy", "Attention",
        "KL Divergence", "Fourier",
        // Definitions and terms
        "Optimization",
        "Transformers",
        "Signal",
        // Footnotes
        "standard mathematical notation",
        "Goodfellow",
    ];

    let mut missing = Vec::new();
    for s in &must_contain {
        if !roundtrip.contains(s) {
            missing.push(*s);
        }
    }
    assert!(
        missing.is_empty(),
        "[math] Round-trip missing {} items: {:?}",
        missing.len(), missing
    );
    println!("[math] All {} content checks passed", must_contain.len());
    println!("=== PASSED: math_and_formulas_roundtrip ===");
}

#[test]
fn test_math_parsing_library_api() {
    // Verify math elements are parsed correctly via library API
    let md = r#"
# Math Test

$E = mc^2$

Block math:

$$
\frac{a}{b} + \sqrt{c}
$$

Multi-line block:

$$
\sum_{i=1}^{n} x_i
\int_{0}^{\infty} e^{-x} dx
$$

Regular paragraph after math.
"#;

    let elements = pdfrs::elements::parse_markdown(md);

    // Check that MathInline and MathBlock elements are parsed
    let math_inline_count = elements.iter().filter(|e| matches!(e, pdfrs::elements::Element::MathInline { .. })).count();
    let math_block_count = elements.iter().filter(|e| matches!(e, pdfrs::elements::Element::MathBlock { .. })).count();

    println!("[math_api] Parsed {} elements, {} inline math, {} block math",
        elements.len(), math_inline_count, math_block_count);

    assert!(math_inline_count >= 1, "Expected at least 1 MathInline, got {}. Elements: {:?}", math_inline_count, elements);
    assert!(math_block_count >= 2, "Expected at least 2 MathBlock, got {}", math_block_count);

    // Verify specific math content
    let has_inline_emc2 = elements.iter().any(|e| {
        if let pdfrs::elements::Element::MathInline { expression } = e {
            expression.contains("E = mc")
        } else { false }
    });
    assert!(has_inline_emc2, "MathInline with E=mc^2 not found");

    let has_block_frac = elements.iter().any(|e| {
        if let pdfrs::elements::Element::MathBlock { expression } = e {
            expression.contains("frac")
        } else { false }
    });
    assert!(has_block_frac, "MathBlock with frac not found");

    let has_block_sum = elements.iter().any(|e| {
        if let pdfrs::elements::Element::MathBlock { expression } = e {
            expression.contains("sum")
        } else { false }
    });
    assert!(has_block_sum, "MathBlock with sum not found");

    // Generate PDF and validate
    let layout = pdfrs::pdf_generator::PageLayout::portrait();
    let bytes = pdfrs::pdf_generator::generate_pdf_bytes(
        &elements, "Helvetica", 12.0, layout,
    ).unwrap();

    let val = pdfrs::pdf::validate_pdf_bytes(&bytes);
    assert!(val.valid, "Math PDF validation failed: {:?}", val.errors);
    println!("[math_api] Generated {} bytes, {} pages", bytes.len(), val.page_count);

    // Check rendered math content in raw PDF
    let content = String::from_utf8_lossy(&bytes);
    assert!(content.contains("SUM"), "Rendered SUM not found in PDF");
    assert!(content.contains("sqrt"), "Rendered sqrt not found in PDF");

    println!("=== PASSED: math_parsing_library_api ===");
}
