//! Incremental PDF updates (append-only saves).
//!
//! Instead of rewriting an entire PDF, an incremental update appends new objects
//! plus a new `xref` / `trailer` / `startxref` / `%%EOF` section that points at
//! the previous xref via `/Prev`. This is how editors and signing tools apply
//! small changes efficiently.
//!
//! # Example
//!
//! ```rust,no_run
//! use pdfrs::incremental::{incremental_set_info, is_incremental_pdf};
//!
//! let original = std::fs::read("doc.pdf").unwrap();
//! let updated = incremental_set_info(&original, Some("New Title"), Some("Ada")).unwrap();
//! assert!(is_incremental_pdf(&updated));
//! ```

use anyhow::{Result, anyhow};

/// Returns true if the PDF appears to contain more than one `%%EOF` (incremental updates).
pub fn is_incremental_pdf(data: &[u8]) -> bool {
    data.windows(5).filter(|w| *w == b"%%EOF").count() > 1
}

/// Locate the byte offset of the last `startxref` value (the previous xref table).
pub fn find_last_xref_offset(data: &[u8]) -> Result<usize> {
    let last_eof = data
        .windows(5)
        .rposition(|w| w == b"%%EOF")
        .ok_or_else(|| anyhow!("PDF missing %%EOF"))?;
    let startxref_pos = data[..last_eof]
        .windows(9)
        .rposition(|w| w == b"startxref")
        .ok_or_else(|| anyhow!("PDF missing startxref"))?;
    let after = &data[startxref_pos + 9..last_eof];
    let s = String::from_utf8_lossy(after);
    let num: usize = s
        .trim()
        .lines()
        .next()
        .and_then(|l| l.trim().parse().ok())
        .ok_or_else(|| anyhow!("Could not parse startxref offset"))?;
    Ok(num)
}

/// Parse `/Size` and `/Root` from the last trailer (best-effort).
fn parse_last_trailer(data: &[u8]) -> Result<(u32, u32)> {
    let last_eof = data
        .windows(5)
        .rposition(|w| w == b"%%EOF")
        .ok_or_else(|| anyhow!("PDF missing %%EOF"))?;
    let startxref_pos = data[..last_eof]
        .windows(9)
        .rposition(|w| w == b"startxref")
        .ok_or_else(|| anyhow!("PDF missing startxref"))?;
    // Trailer usually sits just before startxref
    let search_from = startxref_pos.saturating_sub(512);
    let window = String::from_utf8_lossy(&data[search_from..startxref_pos]);
    let size = regex::Regex::new(r"/Size\s+(\d+)")
        .unwrap()
        .captures(&window)
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(1);
    let root = regex::Regex::new(r"/Root\s+(\d+)\s+\d+\s+R")
        .unwrap()
        .captures(&window)
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(1);
    Ok((size, root))
}

/// Append raw object bodies as an incremental update.
///
/// `objects` entries are `(object_id, full object bytes including "N 0 obj … endobj\n")`.
/// IDs should be ≥ previous `/Size` (or free ids) to avoid clobbering live objects
/// unless an intentional override is desired.
pub fn incremental_append_objects(
    original: &[u8],
    objects: &[(u32, Vec<u8>)],
    new_root: Option<u32>,
    new_info: Option<u32>,
) -> Result<Vec<u8>> {
    if objects.is_empty() {
        return Ok(original.to_vec());
    }
    if !original.starts_with(b"%PDF") {
        return Err(anyhow!("Not a PDF"));
    }

    let prev_xref = find_last_xref_offset(original)?;
    let (old_size, old_root) = parse_last_trailer(original)?;
    let root = new_root.unwrap_or(old_root);

    let mut out = original.to_vec();
    // Ensure we append after the last %%EOF (keep prior bytes intact)
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }

    let update_start = out.len();
    let mut offsets: Vec<(u32, usize)> = Vec::new();

    for (id, body) in objects {
        offsets.push((*id, out.len()));
        out.extend_from_slice(body);
        if !body.ends_with(b"\n") {
            out.push(b'\n');
        }
    }

    let xref_at = out.len();
    // Build a sparse-ish xref: subsection per contiguous run
    offsets.sort_by_key(|(id, _)| *id);
    let mut xref = String::from("xref\n");
    // Free entry for object 0 is conventional in subsections starting at 0;
    // for sparse updates we emit one subsection per object (simple + correct).
    for (id, off) in &offsets {
        xref.push_str(&format!("{id} 1\n{:010} 00000 n \n", *off as u32));
    }
    out.extend_from_slice(xref.as_bytes());

    let max_id = offsets.iter().map(|(id, _)| *id).max().unwrap_or(0);
    let new_size = old_size.max(max_id + 1);

    let mut trailer = format!(
        "trailer\n<< /Size {} /Root {} 0 R /Prev {} ",
        new_size, root, prev_xref
    );
    if let Some(info_id) = new_info {
        trailer.push_str(&format!("/Info {} 0 R ", info_id));
    }
    trailer.push_str(&format!(">>\nstartxref\n{}\n%%EOF\n", xref_at));
    out.extend_from_slice(trailer.as_bytes());

    let _ = update_start; // documented conceptually
    Ok(out)
}

fn escape_info(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// Incrementally set `/Info` metadata (title / author) without rewriting the PDF body.
pub fn incremental_set_info(
    original: &[u8],
    title: Option<&str>,
    author: Option<&str>,
) -> Result<Vec<u8>> {
    let (old_size, _root) = parse_last_trailer(original)?;
    let info_id = old_size; // next free id assuming dense Size

    let mut dict = String::from("<< /Producer (pdfrs) ");
    if let Some(t) = title {
        dict.push_str(&format!("/Title ({}) ", escape_info(t)));
    }
    if let Some(a) = author {
        dict.push_str(&format!("/Author ({}) ", escape_info(a)));
    }
    dict.push_str(">>");

    let body = format!("{} 0 obj\n{}\nendobj\n", info_id, dict);
    incremental_append_objects(
        original,
        &[(info_id, body.into_bytes())],
        None,
        Some(info_id),
    )
}

/// Incrementally append a free-text note as a `/Text` annotation on the first page.
///
/// Uses a high object id and a new page dictionary override referencing the note.
/// For complex page graphs this is a best-effort update aimed at single-page docs.
pub fn incremental_add_text_annotation(
    original: &[u8],
    content: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Result<Vec<u8>> {
    let (old_size, root) = parse_last_trailer(original)?;
    let annot_id = old_size;
    let page_id = old_size + 1;
    let pages_id = old_size + 2;
    let catalog_id = old_size + 3;

    let annot = format!(
        "{} 0 obj\n<< /Type /Annot /Subtype /Text /Rect [{} {} {} {}] /Contents ({}) /Open false >>\nendobj\n",
        annot_id,
        x,
        y,
        x + width,
        y + height,
        escape_info(content)
    );
    // Minimal page that hosts the annotation (MediaBox letter)
    let page = format!(
        "{} 0 obj\n<< /Type /Page /Parent {} 0 R /MediaBox [0 0 612 792] /Annots [{} 0 R] /Resources << >> >>\nendobj\n",
        page_id, pages_id, annot_id
    );
    let pages = format!(
        "{} 0 obj\n<< /Type /Pages /Kids [{} 0 R] /Count 1 >>\nendobj\n",
        pages_id, page_id
    );
    let catalog = format!(
        "{} 0 obj\n<< /Type /Catalog /Pages {} 0 R >>\nendobj\n",
        catalog_id, pages_id
    );

    incremental_append_objects(
        original,
        &[
            (annot_id, annot.into_bytes()),
            (page_id, page.into_bytes()),
            (pages_id, pages.into_bytes()),
            (catalog_id, catalog.into_bytes()),
        ],
        Some(catalog_id),
        None,
    )
    .inspect(|_bytes| {
        let _ = root;
    })
}

/// Write an incremental Info update from file to file.
pub fn incremental_set_info_file(
    input: &str,
    output: &str,
    title: Option<&str>,
    author: Option<&str>,
) -> Result<()> {
    let data = std::fs::read(input)?;
    let updated = incremental_set_info(&data, title, author)?;
    std::fs::write(output, updated)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements::Element;
    use crate::pdf::validate_pdf_bytes;
    use crate::pdf_generator::{PageLayout, generate_pdf_bytes};

    fn sample() -> Vec<u8> {
        let elements = vec![
            Element::Heading {
                level: 1,
                text: "Incremental Base".into(),
            },
            Element::Paragraph {
                text: "Original body.".into(),
            },
        ];
        generate_pdf_bytes(&elements, "Helvetica", 12.0, PageLayout::portrait()).unwrap()
    }

    #[test]
    fn test_incremental_set_info() {
        let original = sample();
        assert!(!is_incremental_pdf(&original));
        let updated =
            incremental_set_info(&original, Some("Updated Title"), Some("Tester")).unwrap();
        assert!(is_incremental_pdf(&updated));
        assert!(updated.len() > original.len());
        assert!(validate_pdf_bytes(&updated).valid);
        let text = String::from_utf8_lossy(&updated);
        assert!(text.contains("/Title (Updated Title)"));
        assert!(text.contains("/Author (Tester)"));
        assert!(text.contains("/Prev "));
        // Original bytes preserved as prefix
        assert_eq!(&updated[..original.len()], &original[..]);
    }

    #[test]
    fn test_incremental_add_annotation() {
        let original = sample();
        let updated =
            incremental_add_text_annotation(&original, "Review note", 72.0, 720.0, 24.0, 24.0)
                .unwrap();
        assert!(is_incremental_pdf(&updated));
        assert!(validate_pdf_bytes(&updated).valid);
        let text = String::from_utf8_lossy(&updated);
        assert!(text.contains("/Subtype /Text"));
        assert!(text.contains("Review note"));
    }

    #[test]
    fn test_find_xref_offset() {
        let pdf = sample();
        let off = find_last_xref_offset(&pdf).unwrap();
        assert!(off > 0);
        assert!(off < pdf.len());
    }
}
