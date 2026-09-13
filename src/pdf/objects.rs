//! Core PDF object model: [`PdfDocument`], [`PdfObject`], and [`PdfValue`]
//! with document loading, reference rewriting, deduplication, and serialization.

use crate::error::Result;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use super::parser::{find_stream_ranges, parse_objects};
use super::{re_obj_ref, re_root};

#[derive(Debug, Clone)]
pub struct PdfDocument {
    pub version: String,
    pub objects: HashMap<u32, PdfObject>,
    pub catalog: u32,
    pub pages: Vec<u32>,
}

#[derive(Debug, Clone)]
pub enum PdfObject {
    Dictionary(HashMap<String, PdfValue>),
    Stream {
        dictionary: HashMap<String, PdfValue>,
        data: Vec<u8>,
    },
    Array(Vec<PdfValue>),
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
    Reference(u32, u32),
    Name(String),
}

#[derive(Debug, Clone)]
pub enum PdfValue {
    Object(PdfObject),
    Reference(u32, u32),
}

fn serialize_value(val: &PdfValue) -> String {
    match val {
        PdfValue::Object(obj) => serialize_object(obj),
        PdfValue::Reference(id, generation) => format!("{} {} R", id, generation),
    }
}

fn serialize_object(obj: &PdfObject) -> String {
    match obj {
        PdfObject::Dictionary(dict) => {
            let mut entries: Vec<String> = Vec::new();
            for (key, value) in dict {
                entries.push(format!("/{} {}", key, serialize_value(value)));
            }
            format!("<< {} >>", entries.join(" "))
        }
        PdfObject::Stream { dictionary, data } => {
            let mut entries: Vec<String> = Vec::new();
            for (key, value) in dictionary {
                entries.push(format!("/{} {}", key, serialize_value(value)));
            }
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
        PdfObject::String(s) => s.clone(),
        PdfObject::Number(n) => {
            if *n == (n.round()) {
                format!("{:.0}", n)
            } else {
                n.to_string()
            }
        }
        PdfObject::Boolean(b) => b.to_string(),
        PdfObject::Null => "null".to_string(),
        PdfObject::Reference(id, generation) => format!("{} {} R", id, generation),
        PdfObject::Name(n) => format!("/{}", n),
    }
}

// --- Document implementation ---

impl Default for PdfDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfDocument {
    pub fn new() -> Self {
        PdfDocument {
            version: "1.4".to_string(),
            objects: HashMap::new(),
            catalog: 0,
            pages: Vec::new(),
        }
    }

    pub fn load_from_file(filename: &str) -> Result<Self> {
        let mut file = File::open(filename)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        Self::load_from_bytes(&buffer)
    }

    /// Parse PDF bytes into a `PdfDocument` without touching the filesystem.
    pub fn load_from_bytes(buffer: &[u8]) -> Result<Self> {
        let content = String::from_utf8_lossy(buffer);
        let mut doc = PdfDocument::new();

        // Parse PDF header
        if let Some(header_line) = content.lines().next()
            && header_line.starts_with("%PDF-")
        {
            doc.version = header_line[5..].to_string();
        }

        // Find all stream data ranges in raw bytes before string parsing corrupts them
        let stream_ranges = find_stream_ranges(buffer);

        parse_objects(&content, &mut doc)?;

        // Replace corrupted stream data with raw bytes from the original buffer.
        // Stream ranges are found in file order; objects must be matched in the
        // same order. We sort by object ID since well-formed PDFs typically store
        // objects (and thus streams) in ascending ID order.
        let mut sorted_obj_ids: Vec<u32> = doc.objects.keys().copied().collect();
        sorted_obj_ids.sort();
        let mut stream_idx = 0;
        for obj_id in sorted_obj_ids {
            if let Some(PdfObject::Stream { data, .. }) = doc.objects.get_mut(&obj_id) {
                if let Some(&(start, end)) = stream_ranges.get(stream_idx) {
                    *data = buffer[start..end].to_vec();
                }
                stream_idx += 1;
            }
        }

        // Parse catalog reference from trailer so to_bytes() can write the correct /Root
        let root_re = re_root();
        if let Some(caps) = root_re.captures(&content)
            && let Ok(id) = caps[1].parse::<u32>()
        {
            doc.catalog = id;
        }

        Ok(doc)
    }

    /// Scan a PdfValue recursively and replace references from `old_id` to `new_id`.
    fn replace_ref_in_value(val: &mut PdfValue, old_id: u32, new_id: u32) {
        match val {
            PdfValue::Object(PdfObject::String(s)) => {
                if let Some(caps) = re_obj_ref().captures(s)
                    && let Ok(id) = caps[1].parse::<u32>()
                    && id == old_id
                {
                    let generation = &caps[2];
                    *s = format!("{} {} R", new_id, generation);
                }
            }
            PdfValue::Object(PdfObject::Dictionary(dict)) => {
                for v in dict.values_mut() {
                    Self::replace_ref_in_value(v, old_id, new_id);
                }
            }
            PdfValue::Object(PdfObject::Array(arr)) => {
                for item in arr.iter_mut() {
                    Self::replace_ref_in_value(item, old_id, new_id);
                }
            }
            PdfValue::Object(PdfObject::Stream { dictionary, .. }) => {
                for v in dictionary.values_mut() {
                    Self::replace_ref_in_value(v, old_id, new_id);
                }
            }
            _ => {}
        }
    }

    /// Replace all references to `old_id` with `new_id` across every object in the document.
    fn replace_references(&mut self, old_id: u32, new_id: u32) {
        for obj in self.objects.values_mut() {
            match obj {
                PdfObject::Dictionary(dict) => {
                    for v in dict.values_mut() {
                        Self::replace_ref_in_value(v, old_id, new_id);
                    }
                }
                PdfObject::Stream { dictionary, .. } => {
                    for v in dictionary.values_mut() {
                        Self::replace_ref_in_value(v, old_id, new_id);
                    }
                }
                PdfObject::Array(arr) => {
                    for item in arr.iter_mut() {
                        Self::replace_ref_in_value(item, old_id, new_id);
                    }
                }
                _ => {}
            }
        }
    }

    /// Build a deterministic content key for an object so exact duplicates can be identified.
    pub(crate) fn object_content_key(obj: &PdfObject) -> Vec<u8> {
        match obj {
            PdfObject::Stream { dictionary, data } => {
                let mut key = Vec::new();
                let mut entries: Vec<(&String, &PdfValue)> = dictionary.iter().collect();
                entries.sort_by_key(|(k, _)| k.as_str());
                for (k, v) in entries {
                    key.extend_from_slice(k.as_bytes());
                    key.push(b':');
                    key.extend_from_slice(serialize_value(v).as_bytes());
                    key.push(b';');
                }
                key.push(b'|');
                key.extend_from_slice(data);
                key
            }
            PdfObject::Dictionary(dict) => {
                let mut key = Vec::new();
                let mut entries: Vec<(&String, &PdfValue)> = dict.iter().collect();
                entries.sort_by_key(|(k, _)| k.as_str());
                for (k, v) in entries {
                    key.extend_from_slice(k.as_bytes());
                    key.push(b':');
                    key.extend_from_slice(serialize_value(v).as_bytes());
                    key.push(b';');
                }
                key
            }
            other => serialize_object(other).into_bytes(),
        }
    }

    /// Remove duplicate objects and rewrite all references to point to a single canonical copy.
    ///
    /// This is most effective after stream recompression has normalized filters and lengths,
    /// so identical streams truly share the same dictionary + data bytes.
    pub fn deduplicate_objects(&mut self) {
        let mut content_map: std::collections::HashMap<Vec<u8>, u32> =
            std::collections::HashMap::new();
        let mut duplicates: Vec<(u32, u32)> = Vec::new(); // (duplicate_id, canonical_id)

        // Sort by ID so the lowest ID is always chosen as the canonical copy
        let mut sorted_ids: Vec<u32> = self.objects.keys().copied().collect();
        sorted_ids.sort();

        for id in sorted_ids {
            let obj = &self.objects[&id];
            let key = Self::object_content_key(obj);
            if let Some(&canonical) = content_map.get(&key) {
                duplicates.push((id, canonical));
            } else {
                content_map.insert(key, id);
            }
        }

        // Update references before removing objects so we still have mutable access
        for (dup_id, canonical_id) in &duplicates {
            self.replace_references(*dup_id, *canonical_id);
        }

        // Remove duplicate objects
        for (dup_id, _) in &duplicates {
            self.objects.remove(dup_id);
        }
    }

    ///
    /// Creates the required /EmbeddedFile stream and /Filespec objects,
    /// then wires them into the document catalog's /Names -> /EmbeddedFiles
    /// name tree so PDF readers can list and open the attachment.
    pub fn embed_file(&mut self, filename: &str, data: &[u8]) -> Result<u32> {
        let next_id = self.objects.keys().copied().max().unwrap_or(0) + 1;

        // 1. Embedded file stream object
        let mut ef_dict = HashMap::new();
        ef_dict.insert(
            "Type".to_string(),
            PdfValue::Object(PdfObject::String("/EmbeddedFile".to_string())),
        );
        ef_dict.insert(
            "Subtype".to_string(),
            PdfValue::Object(PdfObject::String("/application#2Foctet-stream".to_string())),
        );
        ef_dict.insert(
            "Length".to_string(),
            PdfValue::Object(PdfObject::Number(data.len() as f64)),
        );

        let ef_id = next_id;
        self.objects.insert(
            ef_id,
            PdfObject::Stream {
                dictionary: ef_dict,
                data: data.to_vec(),
            },
        );

        // 2. File specification object
        let fs_id = next_id + 1;
        let fs_dict = format!(
            "<< /Type /Filespec /F ({}) /EF << /F {} 0 R >> >>",
            filename, ef_id
        );
        self.objects.insert(fs_id, PdfObject::String(fs_dict));

        // 3. Update catalog to include EmbeddedFiles name tree
        if let Some(PdfObject::Dictionary(catalog_dict)) = self.objects.get_mut(&self.catalog) {
            // Build or update /Names entry
            let names_entry = catalog_dict.entry("Names".to_string()).or_insert_with(|| {
                PdfValue::Object(PdfObject::String(
                    "<< /EmbeddedFiles << /Names [ ] >> >>".to_string(),
                ))
            });

            // We can't easily mutate the string representation, so rebuild it
            // with the new file appended. Format: /Names << /EmbeddedFiles << /Names [ (file1) 5 0 R (file2) 7 0 R ] >> >>
            if let PdfValue::Object(PdfObject::String(existing)) = names_entry {
                // Parse existing entries between /Names [ and ]
                let mut entries = String::new();
                if let Some(start) = existing.find("/Names [")
                    && let Some(end) = existing[start..].find("]")
                {
                    entries = existing[start + 8..start + end].trim().to_string();
                }

                if !entries.is_empty() {
                    entries.push(' ');
                }
                entries.push_str(&format!("({}) {} 0 R", filename, fs_id));

                *existing = format!("<< /EmbeddedFiles << /Names [ {} ] >> >>", entries);
            }
        }

        Ok(fs_id)
    }

    /// Serialize this document back to PDF bytes.
    ///
    /// Writes objects in ascending ID order, builds a fresh xref table,
    /// and produces a minimal but structurally valid PDF.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut pdf = Vec::new();
        pdf.extend_from_slice(format!("%PDF-{}\n", self.version).as_bytes());
        pdf.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");

        let mut offsets = Vec::new();
        let mut current_offset = pdf.len() as u32;

        let mut sorted_ids: Vec<u32> = self.objects.keys().copied().collect();
        sorted_ids.sort();

        for id in &sorted_ids {
            offsets.push(current_offset);
            let obj = &self.objects[id];
            let obj_header = format!("{} 0 obj\n", id);
            pdf.extend_from_slice(obj_header.as_bytes());

            if let PdfObject::Stream { dictionary, data } = obj {
                let mut entries: Vec<String> = Vec::new();
                for (key, value) in dictionary {
                    if key == "Length" {
                        // Ensure /Length matches the actual data size
                        entries.push(format!("/Length {}", data.len()));
                    } else {
                        entries.push(format!("/{} {}", key, serialize_value(value)));
                    }
                }
                let dict_str = format!("<< {} >>\n", entries.join(" "));
                pdf.extend_from_slice(dict_str.as_bytes());
                pdf.extend_from_slice(b"stream\n");
                pdf.extend_from_slice(data);
                pdf.extend_from_slice(b"\nendstream");
            } else {
                pdf.extend_from_slice(serialize_object(obj).as_bytes());
            }
            pdf.extend_from_slice(b"\nendobj\n");
            current_offset = pdf.len() as u32;
        }

        // xref table
        let xref_offset = pdf.len() as u32;
        pdf.extend_from_slice(format!("xref\n0 {}\n", sorted_ids.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets {
            pdf.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
        }

        // trailer
        let root_id = if self.catalog > 0 {
            self.catalog
        } else if let Some(last) = sorted_ids.last() {
            *last
        } else {
            0
        };

        pdf.extend_from_slice(b"trailer\n");
        pdf.extend_from_slice(
            format!(
                "<< /Size {} /Root {} 0 R >>\n",
                sorted_ids.len() + 1,
                root_id
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(b"startxref\n");
        pdf.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        pdf.extend_from_slice(b"%%EOF\n");

        pdf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_load_from_bytes_roundtrip() {
        let elements = vec![
            crate::elements::Element::Heading {
                level: 1,
                text: "Roundtrip".into(),
            },
            crate::elements::Element::Paragraph {
                text: "Testing load_from_bytes.".into(),
            },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        // Parse from bytes
        let doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        assert!(!doc.objects.is_empty());

        // Serialize back to bytes
        let roundtrip_bytes = doc.to_bytes();
        assert!(!roundtrip_bytes.is_empty());

        // Re-parse and verify text is intact
        let doc2 = PdfDocument::load_from_bytes(&roundtrip_bytes).unwrap();
        let text = doc2.get_text().unwrap();
        assert!(
            text.contains("Roundtrip"),
            "Text lost after roundtrip: {}",
            text
        );
        assert!(
            text.contains("Testing load_from_bytes."),
            "Text lost after roundtrip: {}",
            text
        );
    }

    #[test]
    fn test_deduplicate_objects() {
        let mut doc = PdfDocument::new();

        // Insert two identical objects
        doc.objects
            .insert(1, PdfObject::String("shared_content".to_string()));
        doc.objects
            .insert(2, PdfObject::String("shared_content".to_string()));

        // Insert a dictionary that references object 2
        let mut dict = HashMap::new();
        dict.insert(
            "Ref".to_string(),
            PdfValue::Object(PdfObject::String("2 0 R".to_string())),
        );
        doc.objects.insert(3, PdfObject::Dictionary(dict));
        doc.catalog = 3;

        assert_eq!(doc.objects.len(), 3, "Should start with 3 objects");

        doc.deduplicate_objects();

        // Object 2 (duplicate) should be removed; object 1 (canonical) kept
        assert_eq!(doc.objects.len(), 2, "Should remove one duplicate");
        assert!(
            doc.objects.contains_key(&1),
            "Canonical object 1 should remain"
        );
        assert!(
            !doc.objects.contains_key(&2),
            "Duplicate object 2 should be removed"
        );
        assert!(
            doc.objects.contains_key(&3),
            "Referencing object 3 should remain"
        );

        // Reference inside object 3 should now point to 1
        if let PdfObject::Dictionary(d) = &doc.objects[&3] {
            if let PdfValue::Object(PdfObject::String(s)) = &d["Ref"] {
                assert_eq!(s, "1 0 R", "Reference should be rewritten to canonical ID");
            } else {
                panic!("Expected string reference value");
            }
        } else {
            panic!("Expected dictionary object");
        }
    }

    #[test]
    fn test_embed_file_attachment() {
        let elements = vec![crate::elements::Element::Paragraph {
            text: "Document with attachment".into(),
        }];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes =
            crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let mut doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        let original_count = doc.objects.len();

        // Embed a simple text file
        let attachment_data = b"Hello, this is an embedded file!";
        let fs_id = doc.embed_file("test.txt", attachment_data).unwrap();

        // Should have added 2 new objects: embedded file stream + file spec
        assert_eq!(
            doc.objects.len(),
            original_count + 2,
            "Should add 2 objects (embedded file stream + file spec)"
        );

        // Verify the file spec object exists
        assert!(
            doc.objects.contains_key(&fs_id),
            "File spec object should exist"
        );

        // Verify the embedded file stream exists (should be fs_id - 1)
        let ef_id = fs_id - 1;
        assert!(
            doc.objects.contains_key(&ef_id),
            "Embedded file stream object should exist"
        );

        // Verify the catalog was updated with /Names
        if let Some(PdfObject::Dictionary(catalog_dict)) = doc.objects.get(&doc.catalog) {
            assert!(
                catalog_dict.contains_key("Names"),
                "Catalog should contain /Names for embedded files"
            );
        } else {
            panic!("Catalog should be a dictionary");
        }

        // Verify the output PDF serializes correctly
        let output_bytes = doc.to_bytes();
        assert!(
            !output_bytes.is_empty(),
            "PDF with attachment should serialize"
        );

        // Verify /EmbeddedFile appears in output
        let content = String::from_utf8_lossy(&output_bytes);
        assert!(
            content.contains("/EmbeddedFile"),
            "Output should contain /EmbeddedFile type"
        );
        assert!(
            content.contains("/Filespec"),
            "Output should contain /Filespec type"
        );
        assert!(
            content.contains("test.txt"),
            "Output should contain attachment filename"
        );
    }
}
