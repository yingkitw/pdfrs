//! PDF generation from structured document elements
//!
//! This module is the core PDF engine. It takes a vector of [`Element`](crate::elements::Element)s
//! (produced by [`elements::parse_markdown`][`crate::elements::parse_markdown`])
//! and renders them into a standards-compliant PDF byte stream.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use pdfrs::elements;
//! use pdfrs::pdf_generator::{generate_pdf_bytes, PageLayout};
//!
//! let elements = elements::parse_markdown("# Hello\n\nWorld");
//! let layout = PageLayout::portrait();
//! let pdf = generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();
//! ```
//!
//! # Key types
//!
//! - [`PageLayout`] — page size, orientation, and margins
//! - [`PageOrientation`] — `Portrait` or `Landscape`
//! - [`generate_pdf_bytes`] — in-memory PDF generation
//! - [`create_pdf_from_elements_with_layout`] — write to file
//! - [`crate::streaming::StreamingPdfGenerator`] — incremental generation for large documents
//!
//! # Unicode font embedding
//!
//! When the document contains non-ASCII characters, the generator
//! automatically embeds a TrueType font (configurable via the
//! `PDFRS_UNICODE_FONT_PATH` environment variable). Font subsetting
//! can be enabled via [`crate::optimization::OptimizationSettings`] in the [`optimization`](crate::optimization)
//! module.

mod accessibility;
mod assembly;
mod code_highlight;
mod content_stream;
mod fonts;
mod layout;
mod math_layout;
mod objects;
mod pipeline;
mod text_support;
mod unicode_support;

pub use content_stream::OutlineDest;
pub use content_stream::{
    create_pdf, create_pdf_from_elements, create_pdf_from_elements_with_layout,
    create_pdf_from_elements_with_layout_and_compression, create_pdf_with_options,
};
pub use layout::{Color, PageLayout, PageOrientation, PdfVersion, TextAlign};

pub(crate) use text_support::{escape_pdf_string, render_math_text};
#[cfg(test)]
use unicode_support::prepare_unicode_font_support;

pub use objects::{PdfGenerator, PdfObj};
pub use pipeline::{
    generate_pdf_bytes, generate_pdf_bytes_with_compression, generate_pdf_bytes_with_image_base,
    generate_tagged_pdf_bytes, render_page_range,
};
pub(crate) use pipeline::{generate_pdf_bytes_internal, generate_pdf_bytes_internal_with_base};

pub use accessibility::{
    AccessibilityOptions, StructureElement, StructureType, element_to_structure,
};

#[cfg(test)]
mod accessibility_tests {
    use super::*;
    use crate::elements::Element;

    #[test]
    fn test_accessibility_options_default() {
        let opts = AccessibilityOptions::default();
        assert!(!opts.tagged_pdf);
        assert_eq!(opts.language, "en");
        assert!(opts.title.is_none());
    }

    #[test]
    fn test_accessibility_options_builder() {
        let opts = AccessibilityOptions::new()
            .with_tagged_pdf(true)
            .with_language("en-US".to_string())
            .with_title("My Document".to_string());

        assert!(opts.tagged_pdf);
        assert_eq!(opts.language, "en-US");
        assert_eq!(opts.title, Some("My Document".to_string()));
    }

    #[test]
    fn test_structure_type_names() {
        assert_eq!(StructureType::Document.as_pdf_name(), "Document");
        assert_eq!(StructureType::P.as_pdf_name(), "P");
        assert_eq!(StructureType::H1.as_pdf_name(), "H1");
        assert_eq!(StructureType::Figure.as_pdf_name(), "Figure");
    }

    #[test]
    fn test_structure_element_builder() {
        let elem = StructureElement::new(StructureType::P)
            .with_alt_text("A paragraph".to_string())
            .with_actual_text("This is the actual text".to_string());

        assert_eq!(elem.struct_type, StructureType::P);
        assert_eq!(elem.alt_text, Some("A paragraph".to_string()));
        assert_eq!(
            elem.actual_text,
            Some("This is the actual text".to_string())
        );
    }

    #[test]
    fn test_structure_element_with_children() {
        let mut parent = StructureElement::new(StructureType::L);
        parent.add_child(StructureElement::new(StructureType::LI));
        parent.add_child(StructureElement::new(StructureType::LI));

        assert_eq!(parent.children.len(), 2);
    }

    #[test]
    fn test_element_to_structure_heading() {
        let elem = Element::Heading {
            level: 1,
            text: "Hello".into(),
        };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::H1);
        assert_eq!(struct_elem.actual_text, Some("Hello".to_string()));
    }

    #[test]
    fn test_element_to_structure_paragraph() {
        let elem = Element::Paragraph {
            text: "Test paragraph".into(),
        };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::P);
        assert_eq!(struct_elem.actual_text, Some("Test paragraph".to_string()));
    }

    #[test]
    fn test_element_to_structure_code() {
        let elem = Element::CodeBlock {
            language: "rust".into(),
            code: "fn main() {}".into(),
        };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::Code);
        assert_eq!(struct_elem.actual_text, Some("fn main() {}".to_string()));
    }

    #[test]
    fn test_render_math_text_nested_frac_and_left_right() {
        let quadratic = render_math_text(r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}");
        assert!(
            !quadratic.contains('\\'),
            "should not leave raw LaTeX commands: {}",
            quadratic
        );
        assert!(quadratic.contains('±'), "rendered: {}", quadratic);
        assert!(quadratic.contains('√'), "rendered: {}", quadratic);
        assert!(
            quadratic.contains(")/(") || quadratic.contains('⁄'),
            "fraction should render: {}",
            quadratic
        );

        let gaussian = render_math_text(r"\exp\left(-\frac{(x - \mu)^2}{2\sigma^2}\right)");
        assert!(
            !gaussian.contains("≤ft") && !gaussian.contains("\\left"),
            "\\le must not eat \\left: {}",
            gaussian
        );
        assert!(gaussian.contains("exp"), "rendered: {}", gaussian);
    }

    #[test]
    fn test_render_math_text_matrices() {
        let m = render_math_text(r"\begin{bmatrix} a & b \\ c & d \end{bmatrix}");
        assert!(m.contains('[') && m.contains(']'), "rendered: {}", m);
        assert!(m.contains('a') && m.contains('d'), "rendered: {}", m);
        assert!(!m.contains("begin"), "rendered: {}", m);

        let v = render_math_text(r"\begin{vmatrix} \hat{i} & \hat{j} \\ a_1 & a_2 \end{vmatrix}");
        assert!(v.contains('|'), "rendered: {}", v);
        assert!(!v.contains("begin"), "rendered: {}", v);
    }

    #[test]
    fn test_render_math_text_uses_unicode_symbols() {
        let rendered = render_math_text(r"\sum_{i=1}^{n} i \leq n^2 \approx \infty + \sqrt{x}");
        assert!(rendered.contains('∑'));
        assert!(rendered.contains('≤'));
        assert!(rendered.contains('≈'));
        assert!(rendered.contains('∞'));
        assert!(rendered.contains('√'));
    }

    #[test]
    fn test_render_math_text_handles_unbraced_limits() {
        let rendered = render_math_text(r"\int_0^1 x^2 dx + \sum_i^n a_i");
        assert!(
            rendered.contains("∫₀¹") || rendered.contains("∫[0→1]"),
            "rendered: {}",
            rendered
        );
        assert!(
            rendered.contains("∑ᵢⁿ") || rendered.contains("∑[i→n]"),
            "rendered: {}",
            rendered
        );
        assert!(
            rendered.contains("x²") || rendered.contains("x^(2)"),
            "rendered: {}",
            rendered
        );
        assert!(
            rendered.contains("aᵢ") || rendered.contains("a_(i)"),
            "rendered: {}",
            rendered
        );
    }

    #[test]
    fn test_render_math_text_handles_lim_and_to_arrow() {
        let rendered = render_math_text(r"\lim_{x\to0} \frac{\sin x}{x}");
        assert!(rendered.contains("lim(x→0)"), "rendered: {}", rendered);
        assert!(
            rendered.contains("(sin x)/(x)") || rendered.contains("sin x⁄x"),
            "rendered: {}",
            rendered
        );
    }

    #[test]
    fn test_render_math_text_handles_notin_without_partial_in_replacement() {
        let rendered = render_math_text(r"x \notin A");
        assert!(rendered.contains("∉"), "rendered: {}", rendered);
        assert!(!rendered.contains("∈"), "rendered: {}", rendered);
    }

    #[test]
    fn test_render_math_text_handles_set_logic_and_mathbb_symbols() {
        let rendered = render_math_text(
            r"\forall x \in \mathbb{R}, x \subseteq A \land x \notin B \Rightarrow \therefore x \in \mathbb{N}",
        );
        assert!(rendered.contains("∀"), "rendered: {}", rendered);
        assert!(rendered.contains("∈"), "rendered: {}", rendered);
        assert!(rendered.contains("ℝ"), "rendered: {}", rendered);
        assert!(rendered.contains("⊆"), "rendered: {}", rendered);
        assert!(rendered.contains("∧"), "rendered: {}", rendered);
        assert!(rendered.contains("∉"), "rendered: {}", rendered);
        assert!(rendered.contains("⇒"), "rendered: {}", rendered);
        assert!(rendered.contains("∴"), "rendered: {}", rendered);
        assert!(rendered.contains("ℕ"), "rendered: {}", rendered);
    }

    #[test]
    fn test_render_math_text_complex_expressions_with_unicode() {
        let expr1 = render_math_text(r"\int_0^1 x^2 dx + \sum_{i=1}^{n} a_i");
        assert!(
            expr1.contains("∫₀¹"),
            "Should render integral with subscript/superscript: {}",
            expr1
        );
        assert!(expr1.contains("∑"), "Should contain sum symbol: {}", expr1);
        assert!(
            expr1.contains("∑[i=1→n]") || expr1.contains("∑ᵢ"),
            "Sum limits should be readable: {}",
            expr1
        );
        assert!(expr1.contains("x²"), "Should render x squared: {}", expr1);

        let expr2 = render_math_text(r"\prod_{k=1}^{m} b_k");
        assert!(
            expr2.contains("∏"),
            "Should contain product symbol: {}",
            expr2
        );
        assert!(
            expr2.contains("∏[k=1→m]") || expr2.contains("∏ₖ"),
            "Product limits should be readable: {}",
            expr2
        );
        assert!(
            expr2.contains("bₖ") || expr2.contains("b_(k)"),
            "Should render b subscript k: {}",
            expr2
        );

        let expr3 = render_math_text(
            r"\forall x \in \mathbb{R}, x \geq 0 \Rightarrow \sqrt{x} \in \mathbb{R}",
        );
        assert!(expr3.contains("∀"), "Should contain forall: {}", expr3);
        assert!(expr3.contains("∈"), "Should contain element of: {}", expr3);
        assert!(
            expr3.contains("ℝ"),
            "Should contain real numbers: {}",
            expr3
        );
        assert!(
            expr3.contains("≥"),
            "Should contain greater or equal: {}",
            expr3
        );
        assert!(expr3.contains("⇒"), "Should contain implies: {}", expr3);
        assert!(expr3.contains("√"), "Should contain square root: {}", expr3);
    }

    #[test]
    fn test_multi_column_layout_flows_across_columns() {
        let md = r#"<!-- columns:2 -->

## Column Demo

Paragraph one fills the first column with enough words that wrapping and
column advancement can be exercised on a letter-sized page.

Paragraph two continues the story so the layout engine must place text into
the second column after the first column runs out of vertical space.

Paragraph three, four, and five keep adding volume for a visible two-column
spread with a gutter rule between the bands.

- Item alpha
- Item beta
- Item gamma

Final paragraph confirms list content stayed in-flow.

<!-- columns:1 -->

Single column again.
"#;
        let elements = crate::elements::parse_markdown(md);
        let layout = PageLayout::portrait().with_columns(1); // switched via directive
        let bytes = generate_pdf_bytes(&elements, "Helvetica", 11.0, layout).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        let v = crate::pdf::validate_pdf_bytes(&bytes);
        assert!(v.valid, "{:?}", v.errors);
        assert!(v.page_count >= 1);
    }

    #[test]
    fn test_page_layout_column_geometry() {
        let layout = PageLayout::portrait().with_columns(2).with_column_gap(20.0);
        assert_eq!(layout.column_count(), 2);
        let w = layout.column_width();
        assert!((w * 2.0 + 20.0 - layout.full_content_width()).abs() < 0.01);
        assert!((layout.column_left(0) - layout.margin_left).abs() < 0.01);
        assert!((layout.column_left(1) - (layout.margin_left + w + 20.0)).abs() < 0.01);
    }

    #[test]
    fn test_code_block_with_cjk_uses_unicode_font_and_extracts() {
        if prepare_unicode_font_support().is_none() {
            return;
        }
        let elements = vec![
            Element::Heading {
                level: 1,
                text: "CJK Code".into(),
            },
            Element::Paragraph {
                text: "你好 / こんにちは / 안녕하세요".into(),
            },
            Element::CodeBlock {
                language: "rust".into(),
                code: "// 中文注释\nfn hi() { println!(\"こんにちは\"); }\n// 한국어".into(),
            },
        ];
        let bytes = generate_pdf_bytes_internal(
            &elements,
            "Helvetica",
            11.0,
            PageLayout::portrait(),
            None,
            true,
            None,
        )
        .unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        let raw = String::from_utf8_lossy(&bytes);
        assert!(raw.contains("/ToUnicode"));
        // Write + extract
        let path = std::env::temp_dir().join("pdfrs_cjk_code.pdf");
        std::fs::write(&path, &bytes).unwrap();
        let extracted = crate::pdf::extract_text(path.to_str().unwrap()).unwrap();
        assert!(extracted.contains("你好"), "got: {}", extracted);
        assert!(extracted.contains("こんにちは"), "got: {}", extracted);
        assert!(
            extracted.contains("한국어")
                || extracted.contains("중文")
                || extracted.contains("中文"),
            "got: {}",
            extracted
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_unicode_font_encoder_emits_glyph_ids_not_utf16() {
        let Some((bytes, encoder)) = prepare_unicode_font_support() else {
            return;
        };
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        let gid = face.glyph_index('你').unwrap().0;

        let encoded = encoder.encode_text_as_glyph_ids("你");
        let expected = format!("<{:04X}>", gid);

        assert_eq!(encoded, expected);
        assert_ne!(
            encoded, "<4F60>",
            "must not use unicode code point as CID directly"
        );
    }
}

#[cfg(test)]
mod page_range_tests {
    use super::*;
    use crate::elements::Element;

    #[test]
    fn test_render_page_range_extracts_subset() {
        let elements = vec![
            Element::Paragraph {
                text: "First page content".into(),
            },
            Element::PageBreak,
            Element::Paragraph {
                text: "Second page content".into(),
            },
            Element::PageBreak,
            Element::Paragraph {
                text: "Third page content".into(),
            },
        ];
        let layout = PageLayout::portrait();

        // Extract pages 1..3 (second and third pages, 0-indexed)
        let bytes = render_page_range(&elements, "Helvetica", 12.0, layout, 1..3).unwrap();
        assert!(
            !bytes.is_empty(),
            "Rendered page range should produce non-empty PDF"
        );

        // Verify it's a valid PDF
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.starts_with("%PDF-"), "Should be a valid PDF header");

        // Extract text and verify only pages 2 and 3 are present
        let doc = crate::pdf::PdfDocument::load_from_bytes(&bytes).unwrap();
        let text = doc.get_text().unwrap();
        assert!(
            text.contains("Second page content"),
            "Extracted PDF should contain second page text: {}",
            text
        );
        assert!(
            text.contains("Third page content"),
            "Extracted PDF should contain third page text: {}",
            text
        );
        assert!(
            !text.contains("First page content"),
            "Extracted PDF should NOT contain first page text: {}",
            text
        );
    }

    #[test]
    fn test_render_page_range_single_page() {
        let elements = vec![Element::Paragraph {
            text: "Only page".into(),
        }];
        let layout = PageLayout::portrait();

        let bytes = render_page_range(&elements, "Helvetica", 12.0, layout, 0..1).unwrap();
        let doc = crate::pdf::PdfDocument::load_from_bytes(&bytes).unwrap();
        let text = doc.get_text().unwrap();
        assert!(
            text.contains("Only page"),
            "Single page extraction should work: {}",
            text
        );
    }

    #[test]
    fn test_render_page_range_out_of_bounds() {
        let elements = vec![Element::Paragraph {
            text: "One page".into(),
        }];
        let layout = PageLayout::portrait();

        let result = render_page_range(&elements, "Helvetica", 12.0, layout, 5..10);
        assert!(
            result.is_err(),
            "Out-of-bounds range should return an error"
        );
    }

    #[test]
    fn test_generate_tagged_pdf_bytes() {
        use crate::pdf::{validate_pdf_bytes, validate_pdf_ua_bytes};

        let elements = vec![
            Element::Heading {
                level: 1,
                text: "Tagged Document".into(),
            },
            Element::Paragraph {
                text: "This is an accessible PDF.".into(),
            },
        ];
        let layout = PageLayout::portrait();
        let opts = AccessibilityOptions::new()
            .with_tagged_pdf(true)
            .with_language("en-US".to_string())
            .with_title("Test Tagged PDF".to_string());

        let bytes = generate_tagged_pdf_bytes(&elements, "Helvetica", 12.0, layout, opts).unwrap();
        assert!(!bytes.is_empty(), "Should generate non-empty PDF bytes");

        let content = String::from_utf8_lossy(&bytes);

        // Should contain tagged PDF markers
        assert!(content.contains("/MarkInfo"), "Should contain /MarkInfo");
        assert!(
            content.contains("/Marked true"),
            "Should contain /Marked true"
        );
        assert!(
            content.contains("/StructTreeRoot"),
            "Should contain /StructTreeRoot"
        );
        assert!(content.contains("/Lang"), "Should contain /Lang");
        assert!(content.contains("en-US"), "Should contain language");
        assert!(content.contains("Test Tagged PDF"), "Should contain title");

        // Should be structurally valid
        let validation = validate_pdf_bytes(&bytes);
        assert!(
            validation.valid,
            "Tagged PDF should be structurally valid: {:?}",
            validation.errors
        );

        // Should pass PDF/UA structural checks
        let ua = validate_pdf_ua_bytes(&bytes);
        assert!(ua.has_mark_info, "Should have MarkInfo");
        assert!(ua.has_struct_tree, "Should have StructTreeRoot");
        assert!(ua.has_lang, "Should have Lang");
        assert!(ua.has_title, "Should have Title");
        assert!(
            ua.compliant,
            "Tagged PDF should be PDF/UA compliant: {:?}",
            ua.errors
        );
    }

    #[test]
    fn test_generate_tagged_pdf_bytes_disabled() {
        let elements = vec![Element::Paragraph {
            text: "Untagged".into(),
        }];
        let layout = PageLayout::portrait();
        let opts = AccessibilityOptions::new().with_tagged_pdf(false);

        let bytes = generate_tagged_pdf_bytes(&elements, "Helvetica", 12.0, layout, opts).unwrap();
        let content = String::from_utf8_lossy(&bytes);

        // When tagged_pdf is false, should NOT contain tagged markers
        assert!(
            !content.contains("/MarkInfo"),
            "Should not contain /MarkInfo when disabled"
        );
        assert!(
            !content.contains("/StructTreeRoot"),
            "Should not contain /StructTreeRoot when disabled"
        );
    }

    #[test]
    fn test_markdown_image_and_chart_embedding() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let md = "\
# Media\n\n\
![Sample](sample.png)\n\n\
```chart bar\n\
title: Demo\n\
A, 10\n\
B, 20\n\
```\n";
        let elements = crate::elements::parse_markdown(md);
        assert!(elements.iter().any(|e| matches!(e, Element::Image { .. })));
        assert!(elements.iter().any(|e| matches!(e, Element::Chart { .. })));

        let bytes = generate_pdf_bytes_internal_with_base(
            &elements,
            "Helvetica",
            12.0,
            PageLayout::portrait(),
            None,
            false,
            None,
            Some(fixture),
        )
        .unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        let raw = String::from_utf8_lossy(&bytes);
        assert!(raw.contains("/XObject"), "missing image XObject resource");
        assert!(raw.contains("/Subtype /Image") || raw.contains("/Subtype/Image"));
        assert!(raw.contains("/Im1"), "missing Im1 name");
        let validation = crate::pdf::validate_pdf_bytes(&bytes);
        assert!(validation.valid, "{:?}", validation.errors);
    }

    #[test]
    fn test_thesis_toc_citations_and_folios() {
        let md = "\
<!-- pagenumber:roman -->\n\
<!-- toc -->\n\
<!-- pagebreak -->\n\
# Chapter One\n\n\
Body cites [@alpha] and [@beta].\n\n\
<!-- pagebreak -->\n\
<!-- pagenumber:arabic -->\n\
# Chapter Two\n\n\
More text.\n\n\
<!-- bibliography -->\n\
[@alpha]: Alpha, A. (2020). First.\n\
[@beta]: Beta, B. (2021). Second.\n";
        let elements = crate::elements::parse_markdown(md);
        let bytes =
            generate_pdf_bytes(&elements, "Helvetica", 11.0, PageLayout::portrait()).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        let validation = crate::pdf::validate_pdf_bytes(&bytes);
        assert!(validation.valid, "{:?}", validation.errors);
        assert!(validation.page_count >= 2);

        let tmp = std::env::temp_dir().join("pdfrs_thesis_test.pdf");
        std::fs::write(&tmp, &bytes).unwrap();
        let extracted = crate::pdf::extract_text(tmp.to_str().unwrap()).unwrap();
        assert!(
            extracted.contains("Contents"),
            "missing TOC heading: {extracted}"
        );
        assert!(extracted.contains("Chapter One"), "{extracted}");
        assert!(extracted.contains("Bibliography"), "{extracted}");
        assert!(
            extracted.contains("[1]") && extracted.contains("[2]"),
            "missing citation markers: {extracted}"
        );
    }
}
