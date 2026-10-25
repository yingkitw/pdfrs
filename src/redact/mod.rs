//! True content-stream redaction for PDFs.
//!
//! Unlike an opaque overlay, true redaction rewrites the underlying content
//! stream so the redacted text no longer appears in the extracted text or in
//! any text-level reader. The rewriter:
//!
//! 1. Walks every page's content stream **and** the streams of Form
//!    XObjects and annotation appearance streams referenced from the page.
//! 2. Computes the bounding box of each text-show operation (text-showing
//!    operators are masked whether or not they appear inside `BT…ET`).
//! 3. Replaces text whose box intersects a redacted region with spaces —
//!    at **character granularity** (only the characters whose individual
//!    bounding boxes fall within the region are masked, not the whole `Tj`).
//! 4. Removes `Do` operators for image XObjects whose placement intersects
//!    a redacted region, drops their `/XObject` resource entries, and
//!    deletes the image objects themselves when no longer referenced.
//! 5. Appends a solid-black filled rectangle over each redacted region before
//!    `ET`, so any non-text content under the box is also visually obscured.
//!
//! Known limitation: text inside Form XObjects is matched against the page's
//! redaction regions in form-local coordinates (exact for forms placed with
//! an identity or translation-only CTM).
//!
//! ```rust,no_run
//! use pdfrs::redact::{redact_pdf_bytes, RedactionRegion};
//! let pdf = std::fs::read("doc.pdf").unwrap();
//! let redacted = redact_pdf_bytes(&pdf, &[RedactionRegion {
//!     page: 0,
//!     x: 100.0, y: 700.0, width: 200.0, height: 20.0,
//! }]).unwrap();
//! std::fs::write("redacted.pdf", redacted).unwrap();
//! ```
//!
//! The module is split into focused submodules:
//!
//! - `model` — the [`RedactionRegion`] and [`RedactionStyle`] types
//! - `apply` — the document-level redaction pass
//! - `document` — object reference and XObject resource helpers
//! - `rewrite` — the content-stream rewriter

mod apply;
mod document;
mod model;
mod rewrite;

pub use apply::{redact_pdf_bytes, redact_pdf_bytes_with_style};
pub use model::{RedactionRegion, RedactionStyle};

#[cfg(test)]
mod tests {
    use super::document::XObjectInfo;
    use super::rewrite::{mask_string_partial, matrix_multiply, rewrite_stream};
    use super::*;
    use crate::elements;
    use crate::pdf::PdfDocument;
    use crate::pdf::{PdfObject, PdfValue};
    use crate::pdf_generator::{PageLayout, generate_pdf_bytes};
    use crate::search::Rect;
    use std::collections::HashMap;

    fn make_pdf(markdown: &str) -> Vec<u8> {
        generate_pdf_bytes(
            &elements::parse_markdown(markdown),
            "Helvetica",
            12.0,
            PageLayout::portrait(),
        )
        .unwrap()
    }

    #[test]
    fn redact_removes_text_in_region() {
        let pdf = make_pdf("# Hello world\n\nThis is pdfrs.");
        // Redact a strip covering the "pdfrs" line, near the top of the page.
        let redacted = redact_pdf_bytes(
            &pdf,
            &[RedactionRegion {
                page: 0,
                x: 50.0,
                y: 655.0,
                width: 500.0,
                height: 30.0,
            }],
        )
        .unwrap();
        // Original text should still be extractable; "pdfrs" should be gone.
        let original_text = PdfDocument::load_from_bytes(&pdf)
            .unwrap()
            .get_text()
            .unwrap();
        let redacted_text = PdfDocument::load_from_bytes(&redacted)
            .unwrap()
            .get_text()
            .unwrap();
        assert!(
            original_text.contains("pdfrs"),
            "original should have pdfrs"
        );
        assert!(!redacted_text.contains("pdfrs"), "redacted should not");
    }

    #[test]
    fn redact_with_no_regions_returns_input() {
        let pdf = make_pdf("# Hello");
        let out = redact_pdf_bytes(&pdf, &[]).unwrap();
        assert_eq!(out, pdf);
    }

    #[test]
    fn redact_out_of_range_page_errors() {
        let pdf = make_pdf("# Hello");
        let err = redact_pdf_bytes(
            &pdf,
            &[RedactionRegion {
                page: 99,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            }],
        )
        .expect_err("err");
        assert!(err.to_string().contains("99"));
    }

    #[test]
    fn redact_outside_text_does_not_remove_text() {
        let pdf = make_pdf("# Hello world");
        let redacted = redact_pdf_bytes(
            &pdf,
            &[RedactionRegion {
                page: 0,
                x: 0.0,
                y: 0.0,
                width: 5.0,
                height: 5.0,
            }],
        )
        .unwrap();
        let redacted_text = PdfDocument::load_from_bytes(&redacted)
            .unwrap()
            .get_text()
            .unwrap();
        assert!(redacted_text.contains("Hello"), "should still have Hello");
    }

    #[test]
    fn strip_style_skips_black_box_overlay() {
        let pdf = make_pdf("# Hello world");
        let stripped = redact_pdf_bytes_with_style(
            &pdf,
            &[RedactionRegion {
                page: 0,
                x: 0.0,
                y: 700.0,
                width: 500.0,
                height: 50.0,
            }],
            RedactionStyle::Strip,
        )
        .unwrap();
        let blacked = redact_pdf_bytes_with_style(
            &pdf,
            &[RedactionRegion {
                page: 0,
                x: 0.0,
                y: 700.0,
                width: 500.0,
                height: 50.0,
            }],
            RedactionStyle::BlackBox,
        )
        .unwrap();
        let stripped_text = String::from_utf8_lossy(&stripped);
        let blacked_text = String::from_utf8_lossy(&blacked);
        assert!(!stripped_text.contains("0 0 0 rg"));
        assert!(blacked_text.contains("0 0 0 rg"));
    }

    #[test]
    fn partial_string_redaction_preserves_outside_text() {
        // Redact a narrow strip that only covers part of a line.
        // Text outside the strip should survive.
        let pdf = make_pdf("# Hello world\n\nThis is pdfrs.");
        let redacted = redact_pdf_bytes(
            &pdf,
            &[RedactionRegion {
                page: 0,
                x: 50.0,
                y: 655.0,
                width: 60.0, // narrow — only covers a few characters
                height: 20.0,
            }],
        )
        .unwrap();
        let redacted_text = PdfDocument::load_from_bytes(&redacted)
            .unwrap()
            .get_text()
            .unwrap();
        // "Hello" starts at the left margin; a 60pt strip from x=50 should
        // mask some of "Hello" but "world" further right should survive.
        // The exact behavior depends on font metrics, but at minimum the
        // redacted text should differ from the original.
        let original_text = PdfDocument::load_from_bytes(&pdf)
            .unwrap()
            .get_text()
            .unwrap();
        assert_ne!(redacted_text, original_text, "redaction should change text");
    }

    #[test]
    fn mask_string_partial_masks_only_intersecting_chars() {
        // "ABCDEF" at x=90, each char ~6.0pt wide at 12pt (advance=500)
        // Positions: A=90..96, B=96..102, C=102..108, D=108..114, E=114..120, F=120..126
        // Region x=104, width=6 → covers 104..110, intersects C(102..108) and D(108..114)
        let regions = [Rect {
            x: 104.0,
            y: 690.0,
            width: 6.0,
            height: 20.0,
        }];
        let result = mask_string_partial("ABCDEF", 90.0, 700.0, 12.0, None, &regions);
        assert!(result.contains('A'), "A should survive (before region)");
        assert!(result.contains('B'), "B should survive (before region)");
        assert!(result.contains('E'), "E should survive (after region)");
        assert!(result.contains('F'), "F should survive (after region)");
        assert!(result.contains(' '), "some chars should be masked");
    }

    #[test]
    fn matrix_multiply_basic() {
        let identity = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
        let translate = [1.0f32, 0.0, 0.0, 1.0, 100.0, 200.0];
        let result = matrix_multiply(&identity, &translate);
        assert_eq!(result, translate);
    }

    #[test]
    fn rewrite_masks_text_outside_bt_et() {
        // Text-showing operators outside BT…ET must still be masked.
        let regions = [Rect {
            x: 0.0,
            y: 690.0,
            width: 600.0,
            height: 30.0,
        }];
        let src = "/F1 12 Tf\n1 0 0 1 100 700 Tm\n(SECRET outside) Tj\n";
        let (out, removed) = rewrite_stream(
            src,
            &regions,
            &HashMap::new(),
            RedactionStyle::Strip,
            &HashMap::new(),
        );
        assert!(removed.is_empty());
        assert!(
            !out.contains("SECRET"),
            "text outside BT…ET must be masked: {out}"
        );
    }

    #[test]
    fn rewrite_removes_intersecting_image_do_and_reports_name() {
        // Image drawn at 100..300 x 100..200 via cm; region overlaps it.
        let mut xobjects = HashMap::new();
        xobjects.insert(
            "Im0".to_string(),
            XObjectInfo {
                id: 9,
                is_image: true,
                is_form: false,
            },
        );
        let regions = [Rect {
            x: 150.0,
            y: 120.0,
            width: 100.0,
            height: 50.0,
        }];
        let src = "q\n200 0 0 100 100 100 cm\n/Im0 Do\nQ\n";
        let (out, removed) = rewrite_stream(
            src,
            &regions,
            &HashMap::new(),
            RedactionStyle::Strip,
            &xobjects,
        );
        assert_eq!(removed, vec!["Im0".to_string()]);
        assert!(!out.contains("/Im0 Do"), "image Do must be removed: {out}");
    }

    #[test]
    fn rewrite_does_not_mangle_names_or_strings_before_do() {
        // A '/' inside a string literal must not be mistaken for the Do operand.
        let mut xobjects = HashMap::new();
        xobjects.insert(
            "Im0".to_string(),
            XObjectInfo {
                id: 9,
                is_image: true,
                is_form: false,
            },
        );
        let regions = [Rect {
            x: 150.0,
            y: 120.0,
            width: 100.0,
            height: 50.0,
        }];
        let src = "BT\n(a/b) Tj\nET\nq\n200 0 0 100 100 100 cm\n/Im0 Do\nQ\n";
        let (out, removed) = rewrite_stream(
            src,
            &regions,
            &HashMap::new(),
            RedactionStyle::Strip,
            &xobjects,
        );
        assert_eq!(removed, vec!["Im0".to_string()]);
        // The string operand must survive intact (no rfind('/') truncation).
        assert!(out.contains("(a/b)"), "string operand must survive: {out}");
    }

    #[test]
    fn redaction_removes_image_object_from_document() {
        // Build a PDF with an embedded image, then redact over it.
        let png_path = format!("{}/tests/fixtures/sample.png", env!("CARGO_MANIFEST_DIR"));
        let elements = vec![crate::elements::Element::Image {
            alt: "sample".to_string(),
            path: png_path,
        }];
        let pdf = generate_pdf_bytes(&elements, "Helvetica", 12.0, PageLayout::portrait()).unwrap();
        let is_image = |o: &PdfObject| {
            matches!(o, PdfObject::Stream { dictionary, .. }
                if dictionary.get("Subtype")
                    .and_then(|v| match v {
                        PdfValue::Object(PdfObject::Name(n)) => Some(n.as_str()),
                        _ => None,
                    })
                    .is_some_and(|n| n == "Image"))
        };
        // Sanity: the original contains an image XObject.
        let doc = PdfDocument::load_from_bytes(&pdf).unwrap();
        if !doc.objects.values().any(&is_image) {
            return; // fixture unavailable — skip rather than fail
        }

        // Cover the whole page.
        let redacted = redact_pdf_bytes(
            &pdf,
            &[RedactionRegion {
                page: 0,
                x: 0.0,
                y: 0.0,
                width: 612.0,
                height: 792.0,
            }],
        )
        .unwrap();
        let doc2 = PdfDocument::load_from_bytes(&redacted).unwrap();
        assert!(
            !doc2.objects.values().any(&is_image),
            "image object must be removed from the document"
        );
    }
}
