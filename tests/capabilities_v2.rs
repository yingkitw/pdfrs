//! End-to-end integration tests for the v0.2 capability drop:
//! rasterization, search, redaction, full SVG rendering, and structured
//! PDF → Markdown. These complement the unit tests in each module.

use pdfrs::comprehensive::{ComprehensiveOptions, generate_bundled_comprehensive_pdf};
use pdfrs::{elements, pdf_generator, pdf_to_md, raster, redact, search, vector};

fn make_text_pdf(markdown: &str) -> Vec<u8> {
    pdf_generator::generate_pdf_bytes(
        &elements::parse_markdown(markdown),
        "Helvetica",
        12.0,
        pdf_generator::PageLayout::portrait(),
    )
    .expect("generate_pdf_bytes")
}

#[test]
fn rasterize_search_redact_round_trip() {
    // 1. Generate a PDF.
    let pdf = make_text_pdf("# Secret Project\n\nThe launch code is 12345.");

    // 2. Rasterize to PNG and verify the PNG signature.
    let page = raster::rasterize_page(&pdf, 0, 72).expect("rasterize");
    let png = page.to_png().expect("png");
    assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    assert!(page.width == 612 && page.height == 792);

    // 3. Search for the secret.
    let hits = search::search_text(&pdf, "12345", false);
    assert_eq!(hits.len(), 1);
    let secret_bbox = hits[0].bbox;

    // 4. Redact the secret region.
    let redacted = redact::redact_pdf_bytes(
        &pdf,
        &[redact::RedactionRegion {
            page: 0,
            x: secret_bbox.x - 2.0,
            y: secret_bbox.y - 2.0,
            width: secret_bbox.width + 4.0,
            height: secret_bbox.height + 4.0,
        }],
    )
    .expect("redact");

    // 5. Verify the secret is gone from the redacted PDF.
    let after_hits = search::search_text(&redacted, "12345", false);
    assert!(after_hits.is_empty(), "secret should be redacted");

    // 6. Verify the heading is still there.
    let heading_hits = search::search_text(&redacted, "Secret Project", false);
    assert!(!heading_hits.is_empty(), "heading should remain");
}

#[test]
fn full_svg_document_renders_to_valid_pdf() {
    let svg = r##"
<svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
  <g transform="translate(20,20)">
    <rect x="0" y="0" width="120" height="80" fill="#336699" stroke="black" stroke-width="2"/>
    <circle cx="200" cy="40" r="30" fill="#ff0000"/>
    <line x1="0" y1="120" x2="360" y2="120" stroke="#00aa00" stroke-width="1"/>
    <polygon points="50,150 150,150 100,250" fill="#ffcc00" stroke="black"/>
  </g>
</svg>"##;
    let bytes = vector::svg_document_to_pdf_bytes(svg, pdf_generator::PageLayout::portrait())
        .expect("svg_document_to_pdf_bytes");
    let validation = pdfrs::pdf::validate_pdf_bytes(&bytes);
    assert!(validation.valid, "{:?}", validation.errors);
}

#[test]
fn pdf_to_markdown_preserves_structure() {
    let pdf = make_text_pdf(
        "# Title\n\nFirst paragraph.\n\n- Apple\n- Banana\n\n1. One\n2. Two\n\nFinal line.",
    );
    let md = pdf_to_md::pdf_to_markdown_bytes(&pdf).expect("pdf_to_markdown");
    // Headings, bullets, and numbered lists should be reconstructed.
    assert!(
        md.lines()
            .any(|l| l.starts_with('#') && l.contains("Title")),
        "md: {}",
        md
    );
    let bullets = md.matches("- ").count();
    assert!(bullets >= 2, "md: {}", md);
    assert!(md.contains("1. "), "md: {}", md);
    // Plain text content should survive.
    for word in &["First", "paragraph", "Final", "line"] {
        assert!(md.contains(word), "missing word '{}': {}", word, md);
    }
}

#[test]
fn rasterize_multi_page_pdf() {
    // Generate a markdown document with explicit page breaks so we get
    // multiple pages without depending on auto-flow heuristics.
    let mut md = String::from("# Multi-page Test\n\n");
    for i in 0..3 {
        md.push_str(&format!("Content block {} on this page.\n\n", i));
        md.push_str("<!-- pagebreak -->\n\n");
    }
    let pdf = make_text_pdf(&md);
    let pages = raster::rasterize_all(&pdf, 72).expect("rasterize_all");
    assert!(pages.len() >= 2, "expected >=2 pages, got {}", pages.len());
    for p in &pages {
        assert!(p.width >= 600 && p.height >= 700);
    }
}

#[test]
fn search_reports_correct_pages() {
    // Use explicit page breaks so terms land on known pages.
    let mut md = String::new();
    md.push_str("# Page One\n\nApple banana cherry.\n\n");
    md.push_str("<!-- pagebreak -->\n\n");
    md.push_str("# Page Two\n\nDog elephant fox.\n\n");
    let pdf = make_text_pdf(&md);

    let apple_hits = search::search_text(&pdf, "Apple", false);
    assert!(!apple_hits.is_empty());
    let elephant_hits = search::search_text(&pdf, "elephant", true);
    assert!(!elephant_hits.is_empty());
    // Apple is on page 0; elephant is on page 1.
    assert_eq!(apple_hits[0].page, 0);
    assert_eq!(elephant_hits[0].page, 1);
}

#[test]
fn redact_strip_style_removes_text_without_overlay() {
    let pdf = make_text_pdf("Visible text. Hidden text. Visible again.");
    // Locate "Hidden".
    let hits = search::search_text(&pdf, "Hidden", false);
    assert!(!hits.is_empty());
    let bbox = hits[0].bbox;
    let stripped = redact::redact_pdf_bytes_with_style(
        &pdf,
        &[redact::RedactionRegion {
            page: 0,
            x: bbox.x - 2.0,
            y: bbox.y - 2.0,
            width: bbox.width + 4.0,
            height: bbox.height + 4.0,
        }],
        redact::RedactionStyle::Strip,
    )
    .expect("redact");
    // "Hidden" must be gone. Because redaction operates at the Tj granularity,
    // the entire containing string may be replaced with spaces — that is
    // acceptable as long as the secret is unrecoverable.
    let text = pdfrs::pdf::PdfDocument::load_from_bytes(&stripped)
        .unwrap()
        .get_text()
        .unwrap();
    assert!(!text.contains("Hidden"), "text: {}", text);
    // The black-box overlay should NOT appear in strip mode.
    let raw = std::str::from_utf8(stripped.as_slice()).unwrap_or("");
    let stripped_text = raw.to_string();
    assert!(
        !stripped_text.contains("0 0 0 rg"),
        "strip mode should not add black fill"
    );
}

#[test]
fn svg_transform_composition_is_correct() {
    // translate(10,20) then scale(2): point (5,5) should map to (20, 30).
    let m = vector::parse_svg_transform("translate(10,20) scale(2)");
    // matrix composition: result = T * S applied to (5,5)
    // = T(10,20) applied to (S(5,5)=(10,10)) = (20, 30)
    let x = m[0] * 5.0 + m[2] * 5.0 + m[4];
    let y = m[1] * 5.0 + m[3] * 5.0 + m[5];
    assert!((x - 20.0).abs() < 1e-5, "x = {}", x);
    assert!((y - 30.0).abs() < 1e-5, "y = {}", y);
}

/// TOC page numbers for Appendix K must differ across subsections once the
/// fixture carries enough content per K.x. Without per-subsection overflow
/// every K.x collapsed onto the same page (folio "2") and the TOC was
/// uninformative. This test guards that regression.
#[test]
fn comprehensive_appendix_k_toc_has_distinct_page_numbers() {
    let opts = ComprehensiveOptions::default().with_font_size(11.0);
    let pdf = generate_bundled_comprehensive_pdf(&opts).expect("generate comprehensive PDF");

    // Extract the page that holds the TOC entries. We use the bookmarks
    // generated by `pdfcli extract` because the text extraction surface does
    // not separate outline text from body text.
    let tmp = std::env::temp_dir().join("pdfrs_appendix_k_toc.pdf");
    std::fs::write(&tmp, &pdf).unwrap();
    let text = pdfrs::pdf::extract_text(tmp.to_str().unwrap()).expect("extract text");

    // Find the TOC block — bounded by the "Contents" heading and the next
    // roman folio marker ("i") that signals the end of the front matter.
    let toc_start = text
        .find("Contents")
        .expect("TOC heading missing from extracted text");
    let toc_end = text[toc_start..]
        .find("\ni\n")
        .map(|o| toc_start + o)
        .unwrap_or_else(|| text.len());
    let toc_block = &text[toc_start..toc_end];

    // Helper: pull the page-number column off a TOC line.
    // Each line looks like "<title> ... <page>". We split on the last
    // whitespace-separated token after the dot leader.
    let page_of = |title: &str| -> Option<u32> {
        for line in toc_block.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with(title) {
                let last = trimmed.split_whitespace().last()?;
                return last.parse::<u32>().ok();
            }
        }
        None
    };

    let k1 = page_of("K.1").expect("K.1 not in TOC");
    let k2 = page_of("K.2").expect("K.2 not in TOC");
    let k3 = page_of("K.3").expect("K.3 not in TOC");

    assert!(
        k1 < k2 && k2 < k3,
        "Appendix K subsections must land on strictly increasing pages; got K.1={k1}, K.2={k2}, K.3={k3}\nTOC block:\n{toc_block}"
    );

    // Appendix K itself sits at or before K.1 (page 2 in the current fixture).
    let appendix = page_of("Appendix K").expect("Appendix K not in TOC");
    assert!(
        appendix <= k1,
        "Appendix K heading should not appear after its subsections; got Appendix K={appendix}, K.1={k1}"
    );
}
