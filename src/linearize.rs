//! PDF linearization (Fast Web View) for progressive loading.
//!
//! Rewrites a PDF so that:
//! 1. A `/Linearized` parameter dictionary appears immediately after the header
//! 2. The first page and its directly referenced objects are written early
//! 3. Cross-reference and trailer follow at the end with correct `/L` `/E` `/T` `/N` `/O`
//!
//! This enables byte-serving viewers to display the first page sooner. Full ISO
//! hint tables (`/H`) are emitted as a zero-length placeholder; readers that
//! require rich hints still benefit from the early object ordering.

use crate::pdf::{PdfDocument, PdfObject, PdfValue};
use anyhow::{anyhow, Result};
use std::collections::{BTreeSet, HashSet, VecDeque};

/// Returns true if the PDF begins with a `/Linearized` dictionary (Fast Web View).
pub fn is_linearized(data: &[u8]) -> bool {
    let head = if data.len() > 2048 { &data[..2048] } else { data };
    let text = String::from_utf8_lossy(head);
    text.contains("/Linearized")
}

/// Linearize PDF bytes for progressive web viewing.
pub fn linearize_pdf_bytes(pdf_data: &[u8]) -> Result<Vec<u8>> {
    let doc = PdfDocument::load_from_bytes(pdf_data)?;
    write_linearized(&doc)
}

/// Linearize a PDF file to a new path.
pub fn linearize_pdf_file(input: &str, output: &str) -> Result<()> {
    let data = std::fs::read(input)?;
    let out = linearize_pdf_bytes(&data)?;
    std::fs::write(output, out)?;
    Ok(())
}

fn write_linearized(doc: &PdfDocument) -> Result<Vec<u8>> {
    if doc.objects.is_empty() {
        return Err(anyhow!("Cannot linearize an empty PDF"));
    }

    let page_ids = find_page_ids(doc);
    let first_page_id = page_ids.first().copied().unwrap_or(0);
    let n_pages = page_ids.len().max(1);

    let mut priority: Vec<u32> = Vec::new();
    if doc.catalog > 0 {
        priority.push(doc.catalog);
        collect_refs_from_object(doc, doc.catalog, &mut priority, 2);
    }
    if first_page_id > 0 {
        priority.push(first_page_id);
        collect_refs_from_object(doc, first_page_id, &mut priority, 3);
    }
    // Dedupe preserving order
    let mut seen = HashSet::new();
    priority.retain(|id| seen.insert(*id) && doc.objects.contains_key(id));

    let mut rest: Vec<u32> = doc.objects.keys().copied().collect();
    rest.sort();
    rest.retain(|id| !seen.contains(id));

    let lin_id = doc.objects.keys().copied().max().unwrap_or(0) + 1;

    // Placeholders: 10-digit zero-padded fields patched after writing
    const PLACEHOLDER: &str = "0000000000";

    let mut pdf = Vec::new();
    pdf.extend_from_slice(format!("%PDF-{}\n", doc.version).as_bytes());
    pdf.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");

    // Linearization dictionary (must be first object in file)
    let lin_offset = pdf.len();
    let lin_dict = format!(
        "{} 0 obj\n\
         << /Linearized 1.0\n\
         /L {}\n\
         /H [ {} {} ]\n\
         /O {}\n\
         /E {}\n\
         /N {}\n\
         /T {}\n\
         >>\n\
         endobj\n",
        lin_id, PLACEHOLDER, PLACEHOLDER, PLACEHOLDER, first_page_id, PLACEHOLDER, n_pages, PLACEHOLDER
    );
    // Record byte positions of each 10-digit field for patching (L, H0, H1, E, T)
    let field_positions = find_placeholder_positions(lin_offset, &lin_dict);
    pdf.extend_from_slice(lin_dict.as_bytes());

    let mut offsets: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    offsets.insert(lin_id, lin_offset as u32);

    // Write priority (first-page) objects
    for id in &priority {
        let off = pdf.len() as u32;
        offsets.insert(*id, off);
        write_object(&mut pdf, *id, &doc.objects[id]);
    }
    let end_first_page = pdf.len() as u32;

    // Write remaining objects
    for id in &rest {
        let off = pdf.len() as u32;
        offsets.insert(*id, off);
        write_object(&mut pdf, *id, &doc.objects[id]);
    }

    let xref_offset = pdf.len() as u32;

    // Build xref for all object ids including lin_id (dense table 0..max)
    let max_id = *offsets.keys().max().unwrap();
    pdf.extend_from_slice(format!("xref\n0 {}\n", max_id + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for id in 1..=max_id {
        if let Some(off) = offsets.get(&id) {
            pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
        } else {
            pdf.extend_from_slice(b"0000000000 65535 f \n");
        }
    }

    let root_id = if doc.catalog > 0 {
        doc.catalog
    } else {
        first_page_id
    };
    pdf.extend_from_slice(b"trailer\n");
    pdf.extend_from_slice(
        format!("<< /Size {} /Root {} 0 R >>\n", max_id + 1, root_id).as_bytes(),
    );
    pdf.extend_from_slice(b"startxref\n");
    pdf.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    pdf.extend_from_slice(b"%%EOF\n");

    let file_len = pdf.len() as u32;

    // Patch placeholders: order in dict is L, H[0], H[1], E, T
    // (O and N are real numbers, not placeholders)
    if field_positions.len() >= 5 {
        patch_u32(&mut pdf, field_positions[0], file_len); // L
        patch_u32(&mut pdf, field_positions[1], 0); // H offset (no hint stream)
        patch_u32(&mut pdf, field_positions[2], 0); // H length
        patch_u32(&mut pdf, field_positions[3], end_first_page); // E
        patch_u32(&mut pdf, field_positions[4], xref_offset); // T
    }

    Ok(pdf)
}

fn find_placeholder_positions(base: usize, dict: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = dict[search_from..].find("0000000000") {
        let abs = base + search_from + rel;
        positions.push(abs);
        search_from += rel + 10;
    }
    positions
}

fn patch_u32(buf: &mut [u8], at: usize, value: u32) {
    let s = format!("{:010}", value);
    if at + 10 <= buf.len() {
        buf[at..at + 10].copy_from_slice(s.as_bytes());
    }
}

fn write_object(pdf: &mut Vec<u8>, id: u32, obj: &PdfObject) {
    pdf.extend_from_slice(format!("{} 0 obj\n", id).as_bytes());
    match obj {
        PdfObject::Stream { dictionary, data } => {
            let mut entries: Vec<String> = Vec::new();
            for (key, value) in dictionary {
                if key == "Length" {
                    entries.push(format!("/Length {}", data.len()));
                } else {
                    entries.push(format!("/{} {}", key, serialize_value(value)));
                }
            }
            pdf.extend_from_slice(format!("<< {} >>\n", entries.join(" ")).as_bytes());
            pdf.extend_from_slice(b"stream\n");
            pdf.extend_from_slice(data);
            pdf.extend_from_slice(b"\nendstream");
        }
        other => {
            pdf.extend_from_slice(serialize_object(other).as_bytes());
        }
    }
    pdf.extend_from_slice(b"\nendobj\n");
}

fn serialize_value(val: &PdfValue) -> String {
    match val {
        PdfValue::Object(obj) => serialize_object(obj),
        PdfValue::Reference(id, generation) => format!("{} {} R", id, generation),
    }
}

fn escape_pdf_literal(s: &str) -> String {
    s.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")
}

fn serialize_object(obj: &PdfObject) -> String {
    match obj {
        PdfObject::Dictionary(dict) => {
            let entries: Vec<String> = dict
                .iter()
                .map(|(k, v)| format!("/{} {}", k, serialize_value(v)))
                .collect();
            format!("<< {} >>", entries.join(" "))
        }
        PdfObject::Stream { dictionary, data } => {
            let entries: Vec<String> = dictionary
                .iter()
                .map(|(k, v)| {
                    if k == "Length" {
                        format!("/Length {}", data.len())
                    } else {
                        format!("/{} {}", k, serialize_value(v))
                    }
                })
                .collect();
            format!(
                "<< {} >>\nstream\n{}\nendstream",
                entries.join(" "),
                String::from_utf8_lossy(data)
            )
        }
        PdfObject::Array(items) => {
            let parts: Vec<String> = items.iter().map(serialize_value).collect();
            format!("[ {} ]", parts.join(" "))
        }
        PdfObject::String(s) => {
            // Preserve already-encoded PDF tokens; wrap bare literal strings
            // (e.g. outline /Title values) so spaces survive re-serialization.
            if s.starts_with('/')
                || s.starts_with('(')
                || s.starts_with('[')
                || s.starts_with('<')
                || s.starts_with("<<")
            {
                s.clone()
            } else if s.contains(' ') || s.is_empty() {
                format!("({})", escape_pdf_literal(s))
            } else {
                s.clone()
            }
        }
        PdfObject::Number(n) => {
            if n.fract() == 0.0 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        PdfObject::Boolean(b) => {
            if *b {
                "true".into()
            } else {
                "false".into()
            }
        }
        PdfObject::Null => "null".into(),
        PdfObject::Reference(id, generation) => format!("{} {} R", id, generation),
        PdfObject::Name(n) => {
            if n.starts_with('/') {
                n.clone()
            } else {
                format!("/{}", n)
            }
        }
    }
}

fn find_page_ids(doc: &PdfDocument) -> Vec<u32> {
    let mut ids: BTreeSet<u32> = BTreeSet::new();
    for (id, obj) in &doc.objects {
        if object_is_page(obj) {
            ids.insert(*id);
        }
    }
    // Prefer doc.pages if populated
    if !doc.pages.is_empty() {
        return doc.pages.clone();
    }
    ids.into_iter().collect()
}

fn object_is_page(obj: &PdfObject) -> bool {
    let dict = match obj {
        PdfObject::Dictionary(d) => d,
        PdfObject::Stream { dictionary, .. } => dictionary,
        _ => return false,
    };
    match dict.get("Type") {
        Some(PdfValue::Object(PdfObject::String(s))) => {
            let t = s.trim();
            t == "/Page" || t == "Page"
        }
        Some(PdfValue::Object(PdfObject::Name(s))) => {
            let t = s.trim_start_matches('/');
            t == "Page"
        }
        _ => false,
    }
}

fn collect_refs_from_object(doc: &PdfDocument, id: u32, out: &mut Vec<u32>, max_depth: usize) {
    let mut queue: VecDeque<(u32, usize)> = VecDeque::new();
    queue.push_back((id, 0));
    let mut visited = HashSet::new();
    visited.insert(id);

    while let Some((cur, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        let Some(obj) = doc.objects.get(&cur) else {
            continue;
        };
        let mut refs = Vec::new();
        collect_refs_in_object(obj, &mut refs);
        for r in refs {
            if visited.insert(r) && doc.objects.contains_key(&r) {
                out.push(r);
                queue.push_back((r, depth + 1));
            }
        }
    }
}

fn collect_refs_in_object(obj: &PdfObject, refs: &mut Vec<u32>) {
    match obj {
        PdfObject::Dictionary(dict) | PdfObject::Stream { dictionary: dict, .. } => {
            for v in dict.values() {
                collect_refs_in_value(v, refs);
            }
        }
        PdfObject::Array(items) => {
            for v in items {
                collect_refs_in_value(v, refs);
            }
        }
        PdfObject::Reference(id, _) => refs.push(*id),
        _ => {}
    }
}

fn collect_refs_in_value(val: &PdfValue, refs: &mut Vec<u32>) {
    match val {
        PdfValue::Reference(id, _) => refs.push(*id),
        PdfValue::Object(obj) => collect_refs_in_object(obj, refs),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements::Element;
    use crate::pdf_generator::{generate_pdf_bytes, PageLayout};

    fn sample_pdf() -> Vec<u8> {
        let elements = vec![
            Element::Heading {
                level: 1,
                text: "Linearize Me".into(),
            },
            Element::Paragraph {
                text: "First page content.".into(),
            },
            Element::PageBreak,
            Element::Paragraph {
                text: "Second page.".into(),
            },
        ];
        generate_pdf_bytes(&elements, "Helvetica", 12.0, PageLayout::portrait()).unwrap()
    }

    #[test]
    fn test_linearize_adds_dictionary() {
        let original = sample_pdf();
        assert!(!is_linearized(&original));
        let lin = linearize_pdf_bytes(&original).unwrap();
        assert!(is_linearized(&lin));
        assert!(lin.starts_with(b"%PDF"));
        let validation = crate::pdf::validate_pdf_bytes(&lin);
        assert!(validation.valid, "{:?}", validation.errors);
        assert!(validation.page_count >= 2);
    }

    #[test]
    fn test_linearize_idempotent_structure() {
        let original = sample_pdf();
        let once = linearize_pdf_bytes(&original).unwrap();
        let twice = linearize_pdf_bytes(&once).unwrap();
        assert!(is_linearized(&twice));
        assert!(crate::pdf::validate_pdf_bytes(&twice).valid);
    }

    #[test]
    fn test_linearize_file_length_field() {
        let lin = linearize_pdf_bytes(&sample_pdf()).unwrap();
        let text = String::from_utf8_lossy(&lin);
        // /L should equal file length
        let re = regex::Regex::new(r"/L\s+(\d+)").unwrap();
        let caps = re.captures(&text).expect("/L present");
        let declared: usize = caps[1].parse().unwrap();
        assert_eq!(declared, lin.len());
    }
}
