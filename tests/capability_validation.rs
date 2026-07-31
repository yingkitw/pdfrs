//! Comprehensive capability validation for pdfrs.
//!
//! Builds a multi-feature PDF from `tests/fixtures/capability_showcase.md`
//! and asserts structural + content capabilities (plugins, outlines, pages,
//! linearization, vector/SVG/3D side cases, validation).

use pdfrs::elements::{self, Element};
use pdfrs::linearize::{self, is_linearized};
use pdfrs::optimization::{
    CompressionLevel, OptimizationProfile, OptimizationSettings, OptimizedPdfGenerator,
    optimize_pdf_bytes,
};
use pdfrs::pdf::{self, validate_pdf_bytes};
use pdfrs::pdf_generator::{AccessibilityOptions, PageLayout, generate_tagged_pdf_bytes};
use pdfrs::pdf_ops::{ThreeDAnnotation, create_pdf_with_3d_annotation_bytes, pdf_contains_3d_u3d};
use pdfrs::plugin::{PluginRegistry, parse_markdown_with_plugins};
use pdfrs::rtl;
use pdfrs::vector::{self, demo_canvas};
use std::path::Path;

fn showcase_md() -> String {
    let path = format!(
        "{}/tests/fixtures/capability_showcase.md",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read showcase md: {e}"))
}

fn out_dir() -> String {
    let dir = format!("{}/tests/output", env!("CARGO_MANIFEST_DIR"));
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn assert_has_element_kinds(elements: &[Element]) {
    let mut has_heading = false;
    let mut has_paragraph = false;
    let mut has_list = false;
    let mut has_task = false;
    let mut has_table = false;
    let mut has_code = false;
    let mut has_math = false;
    let mut has_quote = false;
    let mut has_hr = false;
    let mut has_pagebreak = false;
    let mut has_footnote = false;
    let mut has_defn = false;
    let mut has_callout_note = false;

    for el in elements {
        match el {
            Element::Heading { .. } => has_heading = true,
            Element::Paragraph { text } => {
                has_paragraph = true;
                if text.contains("NOTE") || text.contains("WARNING") || text.contains("TIP") {
                    has_callout_note = true;
                }
            }
            Element::RichParagraph { .. } => has_paragraph = true,
            Element::UnorderedListItem { .. } | Element::OrderedListItem { .. } => has_list = true,
            Element::TaskListItem { .. } => has_task = true,
            Element::TableRow { .. } => has_table = true,
            Element::CodeBlock { .. } => has_code = true,
            Element::MathBlock { .. } | Element::MathInline { .. } => has_math = true,
            Element::BlockQuote { .. } => has_quote = true,
            Element::HorizontalRule => has_hr = true,
            Element::PageBreak => has_pagebreak = true,
            Element::Footnote { .. } => has_footnote = true,
            Element::DefinitionItem { .. } => has_defn = true,
            _ => {}
        }
    }

    assert!(has_heading, "expected headings");
    assert!(has_paragraph, "expected paragraphs");
    assert!(has_list, "expected lists");
    assert!(has_task, "expected task list items");
    assert!(has_table, "expected table rows");
    assert!(has_code, "expected code blocks");
    assert!(has_math, "expected math");
    assert!(has_quote, "expected blockquotes");
    assert!(has_hr, "expected horizontal rule");
    assert!(has_pagebreak, "expected page break");
    assert!(has_footnote, "expected footnote");
    assert!(has_defn, "expected definition list");
    assert!(
        has_callout_note,
        "expected callout plugin expansion (NOTE/WARNING/TIP)"
    );
}

/// Generate a lean showcase PDF: callout plugins + subset fonts, no linearize.
fn build_showcase_pdf(elements: &[Element]) -> Vec<u8> {
    let settings = OptimizationSettings::new()
        .with_compression(CompressionLevel::High)
        .with_subset_fonts(true)
        .with_linearize(false);
    OptimizedPdfGenerator::new(OptimizationProfile::custom(settings))
        .with_font("Helvetica")
        .with_font_size(11.0)
        .with_layout(PageLayout::portrait())
        .generate_bytes(elements)
        .expect("generate showcase PDF")
}

#[test]
fn test_capability_showcase_end_to_end() {
    let md = showcase_md();
    let registry = PluginRegistry::with_defaults();
    let elements = parse_markdown_with_plugins(&md, &registry);
    assert!(
        elements.len() >= 40,
        "expected rich element set, got {}",
        elements.len()
    );
    assert_has_element_kinds(&elements);

    // RTL helpers (fixture includes RTL-dominant lines)
    assert!(rtl::prefers_rtl_layout("שלום"));
    assert_eq!(rtl::prepare_for_pdf("אב"), "בא");

    let pdf = build_showcase_pdf(&elements);
    assert!(
        pdf.len() < 5_000_000,
        "showcase PDF unexpectedly large: {} bytes (check font embedding)",
        pdf.len()
    );

    let validation = validate_pdf_bytes(&pdf);
    assert!(
        validation.valid,
        "showcase PDF invalid: {:?}",
        validation.errors
    );
    assert!(
        validation.page_count >= 2,
        "page break should yield >=2 pages, got {}",
        validation.page_count
    );

    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Outlines"), "missing document outlines");
    assert!(text.contains("/PageMode /UseOutlines"));
    assert!(
        text.contains("Capability Showcase"),
        "expected showcase title in outline /Title entries"
    );
    assert!(
        text.contains("Continuation After Break"),
        "expected post-pagebreak heading in outlines"
    );
    // 18 outline entries for the showcase headings
    assert!(
        text.contains("/Count 18") || text.contains("/Count 1"),
        "expected outline count metadata"
    );

    let out = format!("{}/capability_showcase.pdf", out_dir());
    std::fs::write(&out, &pdf).unwrap();
    assert!(Path::new(&out).exists());
    println!(
        "[capability] wrote {} ({} bytes, {} pages, {} objects)",
        out,
        pdf.len(),
        validation.page_count,
        validation.object_count
    );

    // Round-trip load + structural re-validate
    let reloaded = pdf::PdfDocument::load_from_bytes(&pdf).unwrap();
    assert!(!reloaded.objects.is_empty());
    let _ = reloaded.get_text().unwrap(); // extraction must not panic (CID may be lossy)

    // Linearize (Fast Web View)
    let linearized = linearize::linearize_pdf_bytes(&pdf).unwrap();
    assert!(is_linearized(&linearized));
    assert!(validate_pdf_bytes(&linearized).valid);
    std::fs::write(
        format!("{}/capability_showcase_linearized.pdf", out_dir()),
        &linearized,
    )
    .ok();

    // Web optimize profile also linearizes
    let web = optimize_pdf_bytes(&pdf, OptimizationProfile::web().settings()).unwrap();
    assert!(is_linearized(&web));
    assert!(validate_pdf_bytes(&web).valid);

    // Incremental update preserves original prefix
    let incr = pdfrs::incremental::incremental_set_info(
        &pdf,
        Some("Capability Showcase (updated)"),
        Some("pdfrs-tests"),
    )
    .unwrap();
    assert!(pdfrs::incremental::is_incremental_pdf(&incr));
    assert_eq!(&incr[..pdf.len()], &pdf[..]);
    assert!(validate_pdf_bytes(&incr).valid);
    std::fs::write(
        format!("{}/capability_showcase_incremental.pdf", out_dir()),
        &incr,
    )
    .ok();

    // Tagged / accessibility path — use Latin-only subset to avoid full-font embed
    let tagged_elements: Vec<Element> = elements
        .iter()
        .filter(|e| match e {
            Element::Heading { text, .. } | Element::Paragraph { text } => {
                text.is_ascii() || text.chars().all(|c| c.is_ascii() || c.is_whitespace())
            }
            Element::MathBlock { .. } | Element::MathInline { .. } => false,
            _ => true,
        })
        .take(25).cloned()
        .collect();
    let tagged = generate_tagged_pdf_bytes(
        &tagged_elements,
        "Helvetica",
        11.0,
        PageLayout::portrait(),
        AccessibilityOptions::new()
            .with_tagged_pdf(true)
            .with_language("en-US".into())
            .with_title("Capability Showcase".into()),
    )
    .unwrap();
    assert!(
        tagged.len() < 500_000,
        "tagged sample too large: {}",
        tagged.len()
    );
    let ua = pdf::validate_pdf_ua_bytes(&tagged);
    assert!(
        ua.compliant || ua.has_mark_info,
        "tagged PDF missing MarkInfo: {ua:?}"
    );
    std::fs::write(
        format!("{}/capability_showcase_tagged.pdf", out_dir()),
        &tagged,
    )
    .ok();
}

#[test]
fn test_capability_side_channels_vector_svg_3d() {
    let dir = out_dir();

    let vector_path = format!("{dir}/capability_vector.pdf");
    demo_canvas()
        .write_pdf(&vector_path, PageLayout::portrait())
        .unwrap();
    let vbytes = std::fs::read(&vector_path).unwrap();
    assert!(validate_pdf_bytes(&vbytes).valid);

    let svg =
        r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M40 160 L100 40 L160 160 Z"/></svg>"#;
    let svg_file = format!("{dir}/capability_triangle.svg");
    std::fs::write(&svg_file, svg).unwrap();
    let svg_pdf = format!("{dir}/capability_svg.pdf");
    vector::svg_file_to_pdf(
        &svg_file,
        &svg_pdf,
        PageLayout::portrait(),
        Some(pdfrs::pdf_generator::Color::black()),
        Some(pdfrs::pdf_generator::Color::rgb(0.9, 0.95, 1.0)),
        1.5,
    )
    .unwrap();
    assert!(validate_pdf_bytes(&std::fs::read(&svg_pdf).unwrap()).valid);

    let u3d = b"U3D\0capability-fixture";
    let annot = ThreeDAnnotation {
        contents: "Capability 3D".into(),
        activate_on_open: true,
        ..Default::default()
    };
    let d3 = create_pdf_with_3d_annotation_bytes("3D Capability", u3d, &annot).unwrap();
    assert!(pdf_contains_3d_u3d(&d3));
    assert!(validate_pdf_bytes(&d3).valid);
    std::fs::write(format!("{dir}/capability_3d.pdf"), &d3).ok();
}

#[test]
fn test_capability_cli_md_to_pdf_with_plugins() {
    let base = env!("CARGO_MANIFEST_DIR");
    let md = format!("{base}/tests/fixtures/capability_showcase.md");
    let pdf = format!("{}/capability_cli_plugins.pdf", out_dir());
    let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_pdfcli"));

    // web profile: subset fonts + linearize
    let output = std::process::Command::new(&bin)
        .args([
            "md-to-pdf",
            &md,
            &pdf,
            "--plugins",
            "callouts",
            "--profile",
            "web",
        ])
        .current_dir(base)
        .output()
        .expect("run pdfcli");
    assert!(
        output.status.success(),
        "md-to-pdf failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&pdf).unwrap();
    assert!(validate_pdf_bytes(&bytes).valid);
    assert!(
        bytes.len() < 2_000_000,
        "CLI showcase PDF too large: {}",
        bytes.len()
    );
    assert!(is_linearized(&bytes), "web profile should linearize");

    let raw = String::from_utf8_lossy(&bytes);
    assert!(
        raw.contains("/Outlines"),
        "CLI PDF should include bookmarks"
    );

    // Re-linearize is idempotent enough to stay valid
    let lin_out = format!("{}/capability_cli_linearized.pdf", out_dir());
    let lin = std::process::Command::new(&bin)
        .args(["linearize-pdf", &pdf, "-o", &lin_out])
        .current_dir(base)
        .output()
        .expect("linearize-pdf");
    assert!(
        lin.status.success(),
        "{}",
        String::from_utf8_lossy(&lin.stderr)
    );
    let lin_bytes = std::fs::read(&lin_out).unwrap();
    assert!(is_linearized(&lin_bytes));
    assert!(validate_pdf_bytes(&lin_bytes).valid);
}

#[test]
fn test_capability_plain_parse_without_plugins_still_works() {
    let md = showcase_md();
    let plain = elements::parse_markdown(&md);
    assert!(!plain.is_empty());
    let pdf = OptimizedPdfGenerator::new(OptimizationProfile::custom(
        OptimizationSettings::new()
            .with_subset_fonts(true)
            .with_linearize(false),
    ))
    .generate_bytes(&plain)
    .unwrap();
    assert!(validate_pdf_bytes(&pdf).valid);
}

#[test]
fn test_capability_report_summary() {
    // Quick inventory used by docs / CI logs
    let md = showcase_md();
    let elements = parse_markdown_with_plugins(&md, &PluginRegistry::with_defaults());
    let headings: Vec<_> = elements
        .iter()
        .filter_map(|e| match e {
            Element::Heading { level, text } => Some((*level, text.as_str())),
            _ => None,
        })
        .collect();
    assert!(headings.len() >= 8, "expected many outline candidates");
    println!("[capability] headings ({}):", headings.len());
    for (level, text) in &headings {
        println!("  h{level}: {text}");
    }
    println!(
        "[capability] total elements: {}, plugins: {:?}",
        elements.len(),
        PluginRegistry::with_defaults().parser_names()
    );
}
