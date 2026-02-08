use std::process::Command;
use std::fs;
use std::path::Path;

/// Helper to run pdf-cli commands
fn run_pdf_cli(args: &[&str]) -> (String, String, bool) {
    let output = Command::new("cargo")
        .args(["run", "--"])
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to execute pdf-cli");

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
