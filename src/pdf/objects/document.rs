//! PdfDocument loading, mutation, embedding, and serialization.

use super::super::parser::{find_stream_ranges, parse_objects};
use super::super::{re_obj_ref, re_root};
use super::types::{PdfDocument, PdfObject, PdfValue, serialize_object, serialize_value};
use crate::error::Result;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

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
