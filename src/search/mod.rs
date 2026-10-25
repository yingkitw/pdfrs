//! Full-text search inside PDFs with per-hit bounding boxes.
//!
//! Walks each page's content stream, computes a bounding rectangle for every
//! text-show operation, and matches the query (case-insensitive substring)
//! against the visible text. Returns one [`SearchHit`] per match — enough
//! information for a viewer to scroll to and highlight the result.
//!
//! The implementation re-uses the same content-stream walker as
//! [`crate::raster`] but tracks per-character positions and emits text events
//! instead of pixels.
//!
//! ```rust,no_run
//! use pdfrs::search::search_text;
//! let pdf = std::fs::read("doc.pdf").unwrap();
//! let hits = search_text(&pdf, "needle", false);
//! for hit in &hits {
//!     println!("page {}: {:?} ({:?})", hit.page, hit.text, hit.bbox);
//! }
//! ```

use crate::pdf::{PdfDocument, PdfObject};

mod metrics;
mod pages;
mod walker;

pub use metrics::{FontMetrics, collect_font_metrics};
pub use pages::{Rect, SearchHit, collect_pages};
pub(crate) use pages::{
    collect_pages_from_doc, decompress_stream, is_deflate_stream, object_dict, page_content_streams,
};
pub(crate) use walker::TjItem;
use walker::{HitCollector, walk_content_stream};
pub(crate) use walker::{extract_font_name, extract_string, extract_tj_array, tokenize};

/// Search the PDF bytes for `query`.
///
/// `case_insensitive` controls substring matching: `false` matches exact case;
/// `true` folds to lowercase before matching. Returns hits in document order.
pub fn search_text(pdf_bytes: &[u8], query: &str, case_insensitive: bool) -> Vec<SearchHit> {
    let doc = match PdfDocument::load_from_bytes(pdf_bytes) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let pages = collect_pages_from_doc(&doc, Some(pdf_bytes));
    let fonts = collect_font_metrics(&doc);
    let tounicode = crate::pdf::collect_tounicode_gid_map(&doc);

    let needle = if case_insensitive {
        query.to_lowercase()
    } else {
        query.to_string()
    };
    if needle.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    for (page_idx, page_id) in pages.iter().enumerate() {
        let content_ids = match page_content_streams(&doc, *page_id) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let mut collector = HitCollector::new(page_idx, &needle, &fonts, case_insensitive);
        for cid in content_ids {
            let raw = match doc.objects.get(&cid) {
                Some(PdfObject::Stream { data, .. }) => data.clone(),
                _ => continue,
            };
            let decompressed = decompress_stream(&raw);
            let text = String::from_utf8_lossy(&decompressed).into_owned();
            walk_content_stream(&text, &mut collector, Some(&tounicode));
        }
        hits.extend(collector.into_hits());
    }
    hits
}

/// Count hits without returning individual rectangles (cheap pre-check).
pub fn count_matches(pdf_bytes: &[u8], query: &str, case_insensitive: bool) -> usize {
    search_text(pdf_bytes, query, case_insensitive).len()
}

// ----- Page collection ----------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements;
    use crate::pdf_generator::{PageLayout, generate_pdf_bytes};

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
    fn finds_single_word() {
        let pdf = make_pdf("# Hello world\n\nThis is pdfrs.");
        let hits = search_text(&pdf, "pdfrs", false);
        assert_eq!(hits.len(), 1, "{:?}", hits);
        assert_eq!(hits[0].text, "pdfrs");
        assert_eq!(hits[0].page, 0);
        assert!(hits[0].bbox.width > 0.0);
        assert!(hits[0].bbox.height > 0.0);
    }

    #[test]
    fn case_insensitive_match() {
        let pdf = make_pdf("# HELLO\n\nworld");
        assert_eq!(search_text(&pdf, "hello", false).len(), 0);
        assert_eq!(search_text(&pdf, "hello", true).len(), 1);
    }

    #[test]
    fn multiple_hits_same_query() {
        let pdf = make_pdf("# foo bar foo\n\nfoo again");
        let hits = search_text(&pdf, "foo", false);
        assert!(hits.len() >= 3, "{} hits", hits.len());
        for hit in &hits {
            assert_eq!(hit.text, "foo");
        }
    }

    #[test]
    fn empty_query_returns_no_hits() {
        let pdf = make_pdf("anything");
        assert!(search_text(&pdf, "", false).is_empty());
    }

    #[test]
    fn count_matches_short_circuits() {
        let pdf = make_pdf("# foo bar foo");
        assert_eq!(count_matches(&pdf, "foo", false), 2);
    }

    #[test]
    fn snippet_contains_match() {
        let pdf = make_pdf("# The quick brown fox jumps over the lazy dog");
        let hits = search_text(&pdf, "fox", false);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("fox"));
    }

    #[test]
    fn rect_intersection() {
        let a = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        let b = Rect {
            x: 5.0,
            y: 5.0,
            width: 10.0,
            height: 10.0,
        };
        assert!(a.intersects(&b));
        let c = Rect {
            x: 100.0,
            y: 100.0,
            width: 5.0,
            height: 5.0,
        };
        assert!(!a.intersects(&c));
    }
}
