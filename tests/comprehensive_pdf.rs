//! Comprehensive PDF generation test case.
//!
//! Builds the bundled multi-feature document, validates structure, writes
//! artifacts under `tests/output/`, and refreshes project-root `comprehensive.pdf`.

use pdfrs::comprehensive::{
    ComprehensiveOptions, generate_bundled_comprehensive_pdf, write_bundled_comprehensive_pdf,
};
use pdfrs::incremental::{incremental_set_info, is_incremental_pdf};
use pdfrs::linearize::is_linearized;
use pdfrs::pdf::validate_pdf_bytes;
use pdfrs::plugin::{PluginRegistry, parse_markdown_with_plugins};
use std::path::Path;

fn out_dir() -> String {
    let dir = format!("{}/tests/output", env!("CARGO_MANIFEST_DIR"));
    std::fs::create_dir_all(&dir).ok();
    dir
}

#[test]
fn test_generate_comprehensive_pdf_document() {
    let opts = ComprehensiveOptions::default().with_font_size(11.0);

    // Element coverage from the bundled markdown + callout plugins
    let md = pdfrs::comprehensive::comprehensive_markdown();
    let elements = parse_markdown_with_plugins(md, &PluginRegistry::with_defaults());
    assert!(
        elements.len() >= 80,
        "comprehensive fixture too small: {} elements",
        elements.len()
    );

    let mut kinds = std::collections::HashSet::new();
    for el in &elements {
        kinds.insert(std::mem::discriminant(el));
    }
    assert!(
        kinds.len() >= 12,
        "expected diverse element kinds, got {}",
        kinds.len()
    );
    use pdfrs::elements::{ChartKind, Element};
    let has = |pred: fn(&Element) -> bool| elements.iter().any(pred);
    assert!(
        has(|e| matches!(e, Element::Columns { .. })),
        "fixture missing Columns"
    );
    assert!(
        has(|e| matches!(
            e,
            Element::Chart {
                kind: ChartKind::Bar | ChartKind::Line | ChartKind::Pie,
                ..
            }
        )),
        "fixture missing Chart"
    );
    assert!(
        has(|e| matches!(e, Element::Image { .. })),
        "fixture missing Image"
    );
    assert!(has(|e| matches!(e, Element::Toc)), "fixture missing Toc");

    let pdf = generate_bundled_comprehensive_pdf(&opts).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    assert!(
        pdf.len() < 2_000_000,
        "comprehensive PDF too large: {} bytes",
        pdf.len()
    );

    let validation = validate_pdf_bytes(&pdf);
    assert!(validation.valid, "{:?}", validation.errors);
    assert!(
        validation.page_count >= 4,
        "expected >=4 pages, got {}",
        validation.page_count
    );

    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("/Outlines"), "missing bookmarks");
    assert!(raw.contains("/PageMode /UseOutlines"));
    assert!(raw.contains("/XObject"), "expected embedded image XObject");
    assert!(
        raw.contains("Comprehensive Document")
            && raw.contains("Part II")
            && raw.contains("Acceptance Criteria"),
        "expected key heading titles in outlines"
    );

    // Round-trip extract must preserve CJK samples from the fixture.
    let tmp = format!("{}/comprehensive_cjk_check.pdf", out_dir());
    std::fs::write(&tmp, &pdf).unwrap();
    let extracted = pdfrs::pdf::extract_text(&tmp).unwrap();
    for needle in [
        "你好世界",
        "こんにちは",
        "안녕하세요",
        "中文注释",
        "한국어 주석",
    ] {
        assert!(
            extracted.contains(needle),
            "missing CJK {:?} in extract:\n{}",
            needle,
            extracted
        );
    }
    std::fs::remove_file(&tmp).ok();

    // Optional linearize path still validates structurally
    let linearized =
        generate_bundled_comprehensive_pdf(&opts.clone().with_linearize(true)).unwrap();
    assert!(is_linearized(&linearized));
    assert!(validate_pdf_bytes(&linearized).valid);

    // Persist for manual review
    let artifact = format!("{}/comprehensive_document.pdf", out_dir());
    std::fs::write(&artifact, &pdf).unwrap();
    assert!(Path::new(&artifact).exists());

    // Refresh convenient project-root sample
    let root_out = format!("{}/comprehensive.pdf", env!("CARGO_MANIFEST_DIR"));
    write_bundled_comprehensive_pdf(&root_out, &opts).unwrap();
    assert!(Path::new(&root_out).exists());

    // Incremental metadata update on top of the comprehensive PDF
    let updated = incremental_set_info(
        &pdf,
        Some("pdfrs Comprehensive Document"),
        Some("capability-test"),
    )
    .unwrap();
    assert!(is_incremental_pdf(&updated));
    assert_eq!(&updated[..pdf.len()], &pdf[..]);
    assert!(validate_pdf_bytes(&updated).valid);
    std::fs::write(
        format!("{}/comprehensive_document_incremental.pdf", out_dir()),
        &updated,
    )
    .ok();

    println!(
        "[comprehensive] {} bytes, {} pages, {} objects -> {}",
        pdf.len(),
        validation.page_count,
        validation.object_count,
        artifact
    );
}

#[test]
fn test_generate_comprehensive_via_cli() {
    let base = env!("CARGO_MANIFEST_DIR");
    let out = format!("{}/tests/output/comprehensive_cli.pdf", base);
    let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_pdfcli"));

    let status = std::process::Command::new(&bin)
        .args(["generate-comprehensive", "-o", &out])
        .current_dir(base)
        .status()
        .expect("run pdfcli generate-comprehensive");
    assert!(status.success());

    let bytes = std::fs::read(&out).unwrap();
    let v = validate_pdf_bytes(&bytes);
    assert!(v.valid, "{:?}", v.errors);
    assert!(v.page_count >= 4);
    let raw = String::from_utf8_lossy(&bytes);
    assert!(raw.contains("Comprehensive Document"));
    assert!(raw.contains("/Outlines"));
}

#[test]
fn test_comprehensive_landscape_option() {
    let opts = ComprehensiveOptions::default()
        .with_landscape(true)
        .with_linearize(false);
    let pdf = generate_bundled_comprehensive_pdf(&opts).unwrap();
    let v = validate_pdf_bytes(&pdf);
    assert!(v.valid, "{:?}", v.errors);
    // Landscape MediaBox is width-major in generated pages
    let raw = String::from_utf8_lossy(&pdf);
    assert!(
        raw.contains("792") && raw.contains("612"),
        "expected landscape media box dimensions"
    );
}
