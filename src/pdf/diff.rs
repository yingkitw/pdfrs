//! Structural PDF comparison: object-level diff with text similarity.

use super::objects::PdfDocument;
use super::re_page;
use crate::error::Result;

/// Structural difference between two PDF documents.
#[derive(Debug, Clone)]
pub struct PdfDiff {
    pub object_count_old: usize,
    pub object_count_new: usize,
    pub pages_old: usize,
    pub pages_new: usize,
    pub text_similarity: f32, // 0.0–1.0, 1.0 = identical text
    pub added_objects: Vec<u32>,
    pub removed_objects: Vec<u32>,
    pub modified_objects: Vec<u32>,
    pub metadata_changed: bool,
    pub has_embedded_files_old: bool,
    pub has_embedded_files_new: bool,
}

/// Compute a structural diff between two PDF byte streams.
///
/// This is useful for version control or regression testing:
/// load two revisions of a PDF and see what changed at the
/// object, page, and text levels.
///
/// # Example
/// ```rust,no_run
/// use pdfrs::pdf::{diff_pdf_bytes, PdfDocument};
///
/// let old = PdfDocument::load_from_file("v1.pdf").unwrap().to_bytes();
/// let new = PdfDocument::load_from_file("v2.pdf").unwrap().to_bytes();
/// let diff = diff_pdf_bytes(&old, &new).unwrap();
/// println!("Added objects: {:?}", diff.added_objects);
/// ```
pub fn diff_pdf_bytes(old: &[u8], new: &[u8]) -> Result<PdfDiff> {
    let old_doc = PdfDocument::load_from_bytes(old)?;
    let new_doc = PdfDocument::load_from_bytes(new)?;

    let object_count_old = old_doc.objects.len();
    let object_count_new = new_doc.objects.len();

    // Count pages by looking for /Type /Page (but not /Pages)
    let old_content = String::from_utf8_lossy(old);
    let new_content = String::from_utf8_lossy(new);
    let page_re = re_page();
    let pages_old = page_re.find_iter(&old_content).count();
    let pages_new = page_re.find_iter(&new_content).count();

    // Compute object-level changes
    let mut added_objects = Vec::new();
    let mut removed_objects = Vec::new();
    let mut modified_objects = Vec::new();

    for id in old_doc.objects.keys() {
        if !new_doc.objects.contains_key(id) {
            removed_objects.push(*id);
        } else if PdfDocument::object_content_key(&old_doc.objects[id])
            != PdfDocument::object_content_key(&new_doc.objects[id])
        {
            modified_objects.push(*id);
        }
    }
    for id in new_doc.objects.keys() {
        if !old_doc.objects.contains_key(id) {
            added_objects.push(*id);
        }
    }

    // Text similarity (simple Jaccard over word sets)
    let old_text = old_doc.get_text().unwrap_or_default();
    let new_text = new_doc.get_text().unwrap_or_default();
    let text_similarity = jaccard_similarity(&old_text, &new_text);

    // Metadata check: compare Info dictionary presence and /Title
    let metadata_changed = {
        let old_has_info = old_content.contains("/Type /Catalog") && old_content.contains("/Info ");
        let new_has_info = new_content.contains("/Type /Catalog") && new_content.contains("/Info ");
        old_has_info != new_has_info
            || old_content.contains("/Title ") != new_content.contains("/Title ")
    };

    let has_embedded_files_old =
        old_content.contains("/EmbeddedFiles") && old_content.contains("/Filespec");
    let has_embedded_files_new =
        new_content.contains("/EmbeddedFiles") && new_content.contains("/Filespec");

    Ok(PdfDiff {
        object_count_old,
        object_count_new,
        pages_old,
        pages_new,
        text_similarity,
        added_objects,
        removed_objects,
        modified_objects,
        metadata_changed,
        has_embedded_files_old,
        has_embedded_files_new,
    })
}

/// Simple Jaccard similarity over whitespace-split words.
fn jaccard_similarity(a: &str, b: &str) -> f32 {
    let set_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let set_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if set_a.is_empty() && set_b.is_empty() {
        return 1.0;
    }
    let intersection: std::collections::HashSet<_> = set_a.intersection(&set_b).collect();
    let union: std::collections::HashSet<_> = set_a.union(&set_b).collect();
    intersection.len() as f32 / union.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_pdf_bytes_detects_changes() {
        let elements_old = vec![crate::elements::Element::Paragraph {
            text: "First version".into(),
        }];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let old_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements_old, "Helvetica", 12.0, layout)
                .unwrap();

        let elements_new = vec![
            crate::elements::Element::Paragraph {
                text: "Second version with more content".into(),
            },
            crate::elements::Element::Paragraph {
                text: "Extra paragraph".into(),
            },
        ];
        let new_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements_new, "Helvetica", 12.0, layout)
                .unwrap();

        let diff = diff_pdf_bytes(&old_bytes, &new_bytes).unwrap();

        // Both should have 1 page
        assert_eq!(diff.pages_old, 1, "Old PDF should have 1 page");
        assert_eq!(diff.pages_new, 1, "New PDF should have 1 page");

        // Text should be somewhat similar but not identical
        assert!(
            diff.text_similarity > 0.0 && diff.text_similarity < 1.0,
            "Text similarity should be between 0 and 1 for partially different docs: {}",
            diff.text_similarity
        );

        // There should be some modified objects (content streams differ)
        assert!(
            !diff.modified_objects.is_empty() || !diff.added_objects.is_empty(),
            "Should detect structural changes between different PDFs"
        );
    }

    #[test]
    fn test_diff_pdf_bytes_identical() {
        let elements = vec![crate::elements::Element::Paragraph {
            text: "Same content".into(),
        }];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let diff = diff_pdf_bytes(&bytes, &bytes).unwrap();

        // Identical PDFs should have 100% text similarity
        assert_eq!(
            diff.text_similarity, 1.0,
            "Identical PDFs should have 100% text similarity"
        );

        // No added, removed, or modified objects
        assert!(
            diff.added_objects.is_empty(),
            "Identical PDFs should have no added objects"
        );
        assert!(
            diff.removed_objects.is_empty(),
            "Identical PDFs should have no removed objects"
        );
        assert!(
            diff.modified_objects.is_empty(),
            "Identical PDFs should have no modified objects"
        );
    }
}
