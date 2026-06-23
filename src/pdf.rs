use crate::compression;
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

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

/// Find all stream data ranges in raw PDF bytes.
/// Returns a list of (data_start, data_end) byte positions for each stream,
/// where data_start..data_end is the raw stream payload (between stream\n and endstream).
fn find_stream_ranges(buffer: &[u8]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let stream_marker = b"\nstream\n";
    let endstream_marker = b"\nendstream";
    let mut pos = 0;

    while let Some(stream_pos) = find_subsequence(&buffer[pos..], stream_marker) {
        let abs_stream = pos + stream_pos;
        let data_start = abs_stream + stream_marker.len();
        // Find the next endstream after this stream marker
        if let Some(end_pos) = find_subsequence(&buffer[data_start..], endstream_marker) {
            let data_end = data_start + end_pos;
            ranges.push((data_start, data_end));
            pos = data_end + endstream_marker.len();
        } else {
            break;
        }
    }

    ranges
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}

// --- Font encoding tables ---

/// WinAnsiEncoding: maps byte values 0x80..0x9F to Unicode codepoints.
/// Standard ASCII range (0x20..0x7F) maps directly.
fn winansi_decode(byte: u8) -> char {
    match byte {
        0x80 => '\u{20AC}', // Euro sign
        0x82 => '\u{201A}', // Single low-9 quotation mark
        0x83 => '\u{0192}', // Latin small f with hook
        0x84 => '\u{201E}', // Double low-9 quotation mark
        0x85 => '\u{2026}', // Horizontal ellipsis
        0x86 => '\u{2020}', // Dagger
        0x87 => '\u{2021}', // Double dagger
        0x88 => '\u{02C6}', // Modifier letter circumflex accent
        0x89 => '\u{2030}', // Per mille sign
        0x8A => '\u{0160}', // Latin capital S with caron
        0x8B => '\u{2039}', // Single left-pointing angle quotation
        0x8C => '\u{0152}', // Latin capital ligature OE
        0x8E => '\u{017D}', // Latin capital Z with caron
        0x91 => '\u{2018}', // Left single quotation mark
        0x92 => '\u{2019}', // Right single quotation mark
        0x93 => '\u{201C}', // Left double quotation mark
        0x94 => '\u{201D}', // Right double quotation mark
        0x95 => '\u{2022}', // Bullet
        0x96 => '\u{2013}', // En dash
        0x97 => '\u{2014}', // Em dash
        0x98 => '\u{02DC}', // Small tilde
        0x99 => '\u{2122}', // Trade mark sign
        0x9A => '\u{0161}', // Latin small s with caron
        0x9B => '\u{203A}', // Single right-pointing angle quotation
        0x9C => '\u{0153}', // Latin small ligature oe
        0x9E => '\u{017E}', // Latin small z with caron
        0x9F => '\u{0178}', // Latin capital Y with diaeresis
        b if b >= 0x20 => b as char,
        _ => '\u{FFFD}', // Replacement character
    }
}

/// MacRomanEncoding: maps byte values 0x80..0xFF to Unicode.
fn macroman_decode(byte: u8) -> char {
    static MACROMAN_HIGH: [char; 128] = [
        '\u{00C4}', '\u{00C5}', '\u{00C7}', '\u{00C9}', '\u{00D1}', '\u{00D6}', '\u{00DC}', '\u{00E1}',
        '\u{00E0}', '\u{00E2}', '\u{00E4}', '\u{00E3}', '\u{00E5}', '\u{00E7}', '\u{00E9}', '\u{00E8}',
        '\u{00EA}', '\u{00EB}', '\u{00ED}', '\u{00EC}', '\u{00EE}', '\u{00EF}', '\u{00F1}', '\u{00F3}',
        '\u{00F2}', '\u{00F4}', '\u{00F6}', '\u{00F5}', '\u{00FA}', '\u{00F9}', '\u{00FB}', '\u{00FC}',
        '\u{2020}', '\u{00B0}', '\u{00A2}', '\u{00A3}', '\u{00A7}', '\u{2022}', '\u{00B6}', '\u{00DF}',
        '\u{00AE}', '\u{00A9}', '\u{2122}', '\u{00B4}', '\u{00A8}', '\u{2260}', '\u{00C6}', '\u{00D8}',
        '\u{221E}', '\u{00B1}', '\u{2264}', '\u{2265}', '\u{00A5}', '\u{00B5}', '\u{2202}', '\u{2211}',
        '\u{220F}', '\u{03C0}', '\u{222B}', '\u{00AA}', '\u{00BA}', '\u{2126}', '\u{00E6}', '\u{00F8}',
        '\u{00BF}', '\u{00A1}', '\u{00AC}', '\u{221A}', '\u{0192}', '\u{2248}', '\u{2206}', '\u{00AB}',
        '\u{00BB}', '\u{2026}', '\u{00A0}', '\u{00C0}', '\u{00C3}', '\u{00D5}', '\u{0152}', '\u{0153}',
        '\u{2013}', '\u{2014}', '\u{201C}', '\u{201D}', '\u{2018}', '\u{2019}', '\u{00F7}', '\u{25CA}',
        '\u{00FF}', '\u{0178}', '\u{2044}', '\u{20AC}', '\u{2039}', '\u{203A}', '\u{FB01}', '\u{FB02}',
        '\u{2021}', '\u{00B7}', '\u{201A}', '\u{201E}', '\u{2030}', '\u{00C2}', '\u{00CA}', '\u{00C1}',
        '\u{00CB}', '\u{00C8}', '\u{00CD}', '\u{00CE}', '\u{00CF}', '\u{00CC}', '\u{00D3}', '\u{00D4}',
        '\u{F8FF}', '\u{00D2}', '\u{00DA}', '\u{00DB}', '\u{00D9}', '\u{0131}', '\u{02C6}', '\u{02DC}',
        '\u{00AF}', '\u{02D8}', '\u{02D9}', '\u{02DA}', '\u{00B8}', '\u{02DD}', '\u{02DB}', '\u{02C7}',
    ];
    if byte < 0x80 {
        byte as char
    } else {
        MACROMAN_HIGH[(byte - 0x80) as usize]
    }
}

/// Decode a byte slice using the specified encoding name
pub fn decode_with_encoding(data: &[u8], encoding: &str) -> String {
    match encoding {
        "WinAnsiEncoding" => data.iter().map(|&b| winansi_decode(b)).collect(),
        "MacRomanEncoding" => data.iter().map(|&b| macroman_decode(b)).collect(),
        _ => String::from_utf8_lossy(data).to_string(),
    }
}

// --- Text positioning tracker ---

/// Tracks cursor position during content stream parsing to detect line breaks
struct TextPositionTracker {
    last_y: f32,
    threshold: f32, // Y movement threshold to insert a newline
}

impl TextPositionTracker {
    fn new() -> Self {
        TextPositionTracker {
            last_y: f32::MAX,
            threshold: 2.0,
        }
    }

    /// Returns true if the Y position changed enough to warrant a newline
    fn moved_to_new_line(&mut self, new_y: f32) -> bool {
        if self.last_y == f32::MAX {
            self.last_y = new_y;
            return false;
        }
        let delta = (self.last_y - new_y).abs();
        self.last_y = new_y;
        delta > self.threshold
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
            && header_line.starts_with("%PDF-") {
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
        let root_re = regex::Regex::new(r"/Root\s+(\d+)\s+\d+\s+R").unwrap();
        if let Some(caps) = root_re.captures(&content)
            && let Ok(id) = caps[1].parse::<u32>() {
                doc.catalog = id;
            }

        Ok(doc)
    }

    /// Scan a PdfValue recursively and replace references from `old_id` to `new_id`.
    fn replace_ref_in_value(val: &mut PdfValue, old_id: u32, new_id: u32) {
        match val {
            PdfValue::Object(PdfObject::String(s)) => {
                if let Some(caps) = regex::Regex::new(r"^(\d+) (\d+) R$").unwrap().captures(s)
                    && let Ok(id) = caps[1].parse::<u32>()
                        && id == old_id {
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
        let mut content_map: std::collections::HashMap<Vec<u8>, u32> = std::collections::HashMap::new();
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

    /// Remove potentially dangerous content from this PDF.
    ///
    /// Sanitization strips:
    /// - JavaScript actions (`/JS`, `/JavaScript`)
    /// - Launch actions (`/S /Launch`)
    /// - External file references (`/F` in stream dictionaries)
    /// - Additional actions (`/AA`)
    /// - OpenAction entries that trigger scripts
    /// - Embedded files that could carry malware
    ///
    /// This is useful when accepting PDFs from untrusted sources or
    /// before publishing them to the web.
    pub fn sanitize(&mut self) {
        // Pass 1: identify dangerous object IDs
        let ids_to_remove: Vec<u32> = self.objects.iter()
            .filter(|(_, obj)| Self::object_is_dangerous(obj))
            .map(|(id, _)| *id)
            .collect();

        // Remove standalone dangerous objects (JS scripts, etc.)
        for id in &ids_to_remove {
            self.objects.remove(id);
        }

        // Pass 2: strip dangerous keys from remaining dictionaries and streams
        for (_, obj) in self.objects.iter_mut() {
            match obj {
                PdfObject::Dictionary(dict) => Self::strip_dangerous_keys(dict),
                PdfObject::Stream { dictionary, .. } => Self::strip_dangerous_keys(dictionary),
                _ => {}
            }
        }

        // Also strip dangerous keys from the catalog
        if let Some(PdfObject::Dictionary(catalog_dict)) = self.objects.get_mut(&self.catalog) {
            catalog_dict.remove("OpenAction");
            catalog_dict.remove("AA");
            catalog_dict.remove("JavaScript");
            catalog_dict.remove("JS");
        }
    }

    /// Check if a PdfObject is inherently dangerous and should be removed entirely.
    fn object_is_dangerous(obj: &PdfObject) -> bool {
        let mut content = String::new();
        match obj {
            PdfObject::Dictionary(dict) | PdfObject::Stream { dictionary: dict, .. } => {
                for (k, v) in dict {
                    content.push_str(k);
                    content.push(' ');
                    content.push_str(&Self::value_to_string(v));
                    content.push(' ');
                }
            }
            PdfObject::String(s) => content.push_str(s),
            _ => {}
        }
        // Standalone JavaScript script objects
        if let PdfObject::Dictionary(dict) = obj
            && (dict.contains_key("JS") || dict.contains_key("JavaScript")) {
                return true;
            }
        // Launch actions or embedded malicious scripts in string form
        if content.contains("/S /Launch") || content.contains("/Launch") {
            return true;
        }
        false
    }

    /// Recursively convert a PdfValue to a string for scanning.
    fn value_to_string(val: &PdfValue) -> String {
        match val {
            PdfValue::Object(obj) => match obj {
                PdfObject::String(s) => s.clone(),
                PdfObject::Number(n) => n.to_string(),
                PdfObject::Boolean(b) => b.to_string(),
                PdfObject::Name(n) => format!("/ {}", n),
                PdfObject::Reference(id, generation) => format!("{} {} R", id, generation),
                PdfObject::Null => "null".to_string(),
                PdfObject::Array(arr) => {
                    let parts: Vec<String> = arr.iter().map(Self::value_to_string).collect();
                    format!("[ {} ]", parts.join(" "))
                }
                PdfObject::Dictionary(dict) => {
                    let parts: Vec<String> = dict.iter()
                        .map(|(k, v)| format!("/{} {}", k, Self::value_to_string(v)))
                        .collect();
                    format!("<< {} >>", parts.join(" "))
                }
                PdfObject::Stream { dictionary, .. } => {
                    let parts: Vec<String> = dictionary.iter()
                        .map(|(k, v)| format!("/{} {}", k, Self::value_to_string(v)))
                        .collect();
                    format!("<< {} >>", parts.join(" "))
                }
            },
            PdfValue::Reference(id, generation) => format!("{} {} R", id, generation),
        }
    }

    /// Strip dangerous keys from a dictionary in-place.
    fn strip_dangerous_keys(dict: &mut HashMap<String, PdfValue>) {
        let dangerous_keys: Vec<String> = dict.keys()
            .filter(|k| {
                let lower = k.to_lowercase();
                lower == "js" ||
                lower == "javascript" ||
                lower == "launch" ||
                lower == "aa" ||
                // Only remove /F from non-Filespec contexts (filespec needs /F for filename)
                // We check the whole dict to see if it's a Filespec
                (lower == "f" && !dict.contains_key("Type"))
            })
            .cloned()
            .collect();

        for key in dangerous_keys {
            dict.remove(&key);
        }

        // Recursively sanitize nested dictionaries
        for val in dict.values_mut() {
            if let PdfValue::Object(PdfObject::Dictionary(inner)) = val {
                Self::strip_dangerous_keys(inner);
            }
            if let PdfValue::Object(PdfObject::Stream { dictionary, .. }) = val {
                Self::strip_dangerous_keys(dictionary);
            }
            if let PdfValue::Object(PdfObject::Array(arr)) = val {
                for item in arr.iter_mut() {
                    if let PdfValue::Object(PdfObject::Dictionary(inner)) = item {
                        Self::strip_dangerous_keys(inner);
                    }
                    if let PdfValue::Object(PdfObject::Stream { dictionary, .. }) = item {
                        Self::strip_dangerous_keys(dictionary);
                    }
                }
            }
        }
    }

    /// Embed an external file into this PDF as an attachment.
    ///
    /// Creates the required /EmbeddedFile stream and /Filespec objects,
    /// then wires them into the document catalog's /Names -> /EmbeddedFiles
    /// name tree so PDF readers can list and open the attachment.
    pub fn embed_file(&mut self, filename: &str, data: &[u8]) -> Result<u32> {
        let next_id = self.objects.keys().copied().max().unwrap_or(0) + 1;

        // 1. Embedded file stream object
        let mut ef_dict = HashMap::new();
        ef_dict.insert("Type".to_string(), PdfValue::Object(PdfObject::String("/EmbeddedFile".to_string())));
        ef_dict.insert("Subtype".to_string(), PdfValue::Object(PdfObject::String("/application#2Foctet-stream".to_string())));
        ef_dict.insert("Length".to_string(), PdfValue::Object(PdfObject::Number(data.len() as f64)));

        let ef_id = next_id;
        self.objects.insert(ef_id, PdfObject::Stream {
            dictionary: ef_dict,
            data: data.to_vec(),
        });

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
                PdfValue::Object(PdfObject::String("<< /EmbeddedFiles << /Names [ ] >> >>".to_string()))
            });

            // We can't easily mutate the string representation, so rebuild it
            // with the new file appended. Format: /Names << /EmbeddedFiles << /Names [ (file1) 5 0 R (file2) 7 0 R ] >> >>
            if let PdfValue::Object(PdfObject::String(existing)) = names_entry {
                // Parse existing entries between /Names [ and ]
                let mut entries = String::new();
                if let Some(start) = existing.find("/Names [")
                    && let Some(end) = existing[start..].find("]") {
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
        pdf.extend_from_slice(format!("<< /Size {} /Root {} 0 R >>\n", sorted_ids.len() + 1, root_id).as_bytes());
        pdf.extend_from_slice(b"startxref\n");
        pdf.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        pdf.extend_from_slice(b"%%EOF\n");

        pdf
    }

    pub fn get_text(&self) -> Result<String> {
        let mut text = String::new();
        // Matches (text) Tj — single string show
        let tj_re = regex::Regex::new(r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)\s*Tj").unwrap();
        // Matches <hex> Tj — hex string show
        let tj_hex_re = regex::Regex::new(r"<([0-9a-fA-F\s]+)>\s*Tj").unwrap();
        // Matches [...] TJ — array show (strings + kerning numbers)
        let tj_array_re = regex::Regex::new(r"\[((?:[^\]]*?))\]\s*TJ").unwrap();
        // Matches string elements inside a TJ array
        let tj_str_re = regex::Regex::new(r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)").unwrap();
        // Matches hex string elements inside a TJ array
        let tj_hex_str_re = regex::Regex::new(r"<([0-9a-fA-F\s]+)>").unwrap();
        // Matches Td/TD positioning operators: <x> <y> Td
        let td_re = regex::Regex::new(r"([\d.\-]+)\s+([\d.\-]+)\s+T[dD]").unwrap();
        // Matches Tm text matrix: a b c d e f Tm (f = y position)
        let tm_re = regex::Regex::new(r"[\d.\-]+\s+[\d.\-]+\s+[\d.\-]+\s+[\d.\-]+\s+([\d.\-]+)\s+([\d.\-]+)\s+Tm").unwrap();

        // Sort objects by ID to maintain page order
        let mut sorted_ids: Vec<&u32> = self.objects.keys().collect();
        sorted_ids.sort();

        for obj_id in sorted_ids {
            let obj = &self.objects[obj_id];
            if let PdfObject::Stream { data, .. } = obj {
                let processed_data = decompress_stream(data);
                let content = String::from_utf8_lossy(&processed_data);

                let mut tracker = TextPositionTracker::new();
                let mut first_item_on_line = true;

                // Process content stream line by line to track positioning
                for line in content.lines() {
                    let line = line.trim();

                    // Check for Td/TD positioning BEFORE extracting text on this line
                    if let Some(caps) = td_re.captures(line)
                        && let Ok(y) = caps[2].parse::<f32>()
                            && tracker.moved_to_new_line(y) && !text.ends_with('\n') {
                                // Y changed significantly — likely a new line
                                text.push('\n');
                                first_item_on_line = true;
                            }

                    // Check for Tm text matrix BEFORE extracting text on this line
                    if let Some(caps) = tm_re.captures(line)
                        && let Ok(y) = caps[2].parse::<f32>()
                            && tracker.moved_to_new_line(y) && !text.ends_with('\n') {
                                // Y changed significantly
                                text.push('\n');
                                first_item_on_line = true;
                            }

                    // Extract (text) Tj
                    for caps in tj_re.captures_iter(line) {
                        let extracted = &caps[1];
                        let unescaped = unescape_pdf_string(extracted);
                        if !first_item_on_line && !text.ends_with(' ') && !text.ends_with('\n') {
                            text.push(' ');
                        }
                        text.push_str(&unescaped);
                        first_item_on_line = false;
                    }

                    // Extract <hex> Tj
                    for caps in tj_hex_re.captures_iter(line) {
                        let hex_str = caps[1].replace(char::is_whitespace, "");
                        let decoded = decode_pdf_hex_string(&hex_str);
                        if !first_item_on_line && !text.ends_with(' ') && !text.ends_with('\n') {
                            text.push(' ');
                        }
                        text.push_str(&decoded);
                        first_item_on_line = false;
                    }

                    // Extract [...] TJ arrays
                    for caps in tj_array_re.captures_iter(line) {
                        let array_content = &caps[1];
                        
                        // Extract regular strings
                        for str_caps in tj_str_re.captures_iter(array_content) {
                            let extracted = &str_caps[1];
                            let unescaped = unescape_pdf_string(extracted);
                            if !first_item_on_line && !text.ends_with(' ') && !text.ends_with('\n') {
                                text.push(' ');
                            }
                            text.push_str(&unescaped);
                            first_item_on_line = false;
                        }
                        
                        // Extract hex strings
                        for hex_caps in tj_hex_str_re.captures_iter(array_content) {
                            let hex_str = hex_caps[1].replace(char::is_whitespace, "");
                            let decoded = decode_pdf_hex_string(&hex_str);
                            if !first_item_on_line && !text.ends_with(' ') && !text.ends_with('\n') {
                                text.push(' ');
                            }
                            text.push_str(&decoded);
                            first_item_on_line = false;
                        }
                    }
                }

                // Add newline at the end of each page's content
                if !text.ends_with('\n') && !text.is_empty() {
                    text.push('\n');
                }
            }
        }

        Ok(text)
    }
}

/// Check if bytes form a valid zlib header (CMF=0x78, FLG satisfies checksum)
fn is_zlib_header(b0: u8, b1: u8) -> bool {
    b0 == 0x78 && ((b0 as u16) * 256 + (b1 as u16)).is_multiple_of(31)
}

/// Decompress stream data if it appears to be deflate-compressed
fn decompress_stream(data: &[u8]) -> Vec<u8> {
    if data.len() > 2 && is_zlib_header(data[0], data[1]) {
        match compression::decompress_deflate(data) {
            Ok(decompressed) => decompressed,
            Err(_) => data.to_vec(),
        }
    } else {
        data.to_vec()
    }
}

// --- Object parsing ---

fn parse_objects(content: &str, doc: &mut PdfDocument) -> Result<()> {
    let obj_re = regex::Regex::new(r"(\d+)\s+(\d+)\s+obj\b").unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        if let Some(caps) = obj_re.captures(line) {
            // Only match if the line is exactly "N G obj" (possibly with trailing whitespace)
            let full_match = caps.get(0).unwrap().as_str();
            if (line == full_match || line.starts_with(full_match))
                && let (Ok(obj_num), Ok(_gen_num)) =
                    (caps[1].parse::<u32>(), caps[2].parse::<u32>())
                {
                    i += 1;
                    let mut obj_content = String::new();

                    while i < lines.len() && !lines[i].trim().starts_with("endobj") {
                        obj_content.push_str(lines[i]);
                        obj_content.push('\n');
                        i += 1;
                    }

                    let obj = parse_object_content(&obj_content)?;
                    doc.objects.insert(obj_num, obj);
                }
        }
        i += 1;
    }

    Ok(())
}

fn parse_object_content(content: &str) -> Result<PdfObject> {
    let content = content.trim();

    // Check for stream objects: dictionary followed by stream data
    if let (Some(stream_pos), Some(endstream_pos)) =
        (content.find("\nstream\n"), content.find("\nendstream"))
    {
        let dict_part = content[..stream_pos].trim();
        let data_start = stream_pos + "\nstream\n".len();
        let data = content[data_start..endstream_pos].as_bytes().to_vec();

        let dict = parse_dict_entries(dict_part);

        Ok(PdfObject::Stream {
            dictionary: dict,
            data,
        })
    } else if content.contains("stream") && content.contains("endstream") {
        let stream_idx = content.find("stream").unwrap();
        let endstream_idx = content.find("endstream").unwrap();
        let data_start = stream_idx + "stream".len();
        let data = content[data_start..endstream_idx]
            .trim()
            .as_bytes()
            .to_vec();

        Ok(PdfObject::Stream {
            dictionary: HashMap::new(),
            data,
        })
    } else if content.starts_with("<<") && content.ends_with(">>") {
        let dict = parse_dict_entries(content);
        Ok(PdfObject::Dictionary(dict))
    } else if content.starts_with('[') && content.ends_with(']') {
        let array_content = &content[1..content.len() - 1];
        let items = array_content
            .split_whitespace()
            .map(|item| PdfValue::Object(PdfObject::String(item.to_string())))
            .collect();
        Ok(PdfObject::Array(items))
    } else if content.starts_with('(') && content.ends_with(')') {
        Ok(PdfObject::String(
            content[1..content.len() - 1].to_string(),
        ))
    } else {
        Ok(PdfObject::String(content.to_string()))
    }
}

/// Parse dictionary entries from << ... >> content
fn parse_dict_entries(raw: &str) -> HashMap<String, PdfValue> {
    let mut dict = HashMap::new();
    let inner = raw
        .trim()
        .trim_start_matches("<<")
        .trim_end_matches(">>");
    let tokens: Vec<&str> = inner.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].starts_with('/') {
            let key = tokens[i][1..].to_string();
            i += 1;
            if i < tokens.len() {
                let val = tokens[i].to_string();
                dict.insert(
                    key,
                    PdfValue::Object(PdfObject::String(val)),
                );
            }
        }
        i += 1;
    }
    dict
}

/// Parse a cross-reference stream (PDF 1.5+).
///
/// XRef streams replace the traditional `xref` table with a compressed stream
/// containing object offsets. The /W array specifies field widths.
/// Returns a list of (obj_num, field2, field3) where:
///   type 0: free object (field2=next_free, field3=gen)
///   type 1: normal object (field2=byte_offset, field3=gen)
///   type 2: compressed object (field2=obj_stream_num, field3=index_in_stream)
pub fn parse_xref_stream(data: &[u8], w_fields: &[usize], size: usize) -> Vec<(usize, u64, u64)> {
    let mut entries = Vec::new();
    if w_fields.len() < 3 {
        return entries;
    }

    let entry_size = w_fields[0] + w_fields[1] + w_fields[2];
    if entry_size == 0 {
        return entries;
    }

    let mut pos = 0;
    let mut obj_num = 0;

    while pos + entry_size <= data.len() && obj_num < size {
        let field_type = read_xref_field(data, pos, w_fields[0]);
        let field2 = read_xref_field(data, pos + w_fields[0], w_fields[1]);
        let field3 = read_xref_field(data, pos + w_fields[0] + w_fields[1], w_fields[2]);

        let _ = field_type; // used by caller to interpret field2/field3
        entries.push((obj_num, field2, field3));

        pos += entry_size;
        obj_num += 1;
    }

    entries
}

/// Read a big-endian integer field of `width` bytes from `data` at `offset`.
fn read_xref_field(data: &[u8], offset: usize, width: usize) -> u64 {
    if width == 0 {
        return 0;
    }
    let mut value: u64 = 0;
    for i in 0..width {
        if offset + i < data.len() {
            value = (value << 8) | data[offset + i] as u64;
        }
    }
    value
}

/// Parse an object stream (/Type /ObjStm).
///
/// Object streams contain multiple compressed objects. The stream starts with
/// N pairs of (obj_num, byte_offset) followed by the object data.
/// `first` is the byte offset of the first object's data within the stream.
pub fn parse_object_stream(data: &[u8], n: usize, first: usize) -> Vec<(u32, String)> {
    let mut results = Vec::new();
    let content = String::from_utf8_lossy(data);

    // Parse the header: N pairs of (obj_num offset)
    let header = if first <= content.len() {
        &content[..first]
    } else {
        return results;
    };

    let tokens: Vec<&str> = header.split_whitespace().collect();
    if tokens.len() < n * 2 {
        return results;
    }

    let mut obj_entries: Vec<(u32, usize)> = Vec::new();
    for i in 0..n {
        let obj_num = tokens[i * 2].parse::<u32>().unwrap_or(0);
        let offset = tokens[i * 2 + 1].parse::<usize>().unwrap_or(0);
        obj_entries.push((obj_num, offset));
    }

    // Extract each object's content
    let obj_data = if first <= content.len() {
        &content[first..]
    } else {
        return results;
    };

    for (idx, (obj_num, offset)) in obj_entries.iter().enumerate() {
        let start = *offset;
        let end = if idx + 1 < obj_entries.len() {
            obj_entries[idx + 1].1
        } else {
            obj_data.len()
        };

        if start <= obj_data.len() && end <= obj_data.len() && start <= end {
            let obj_content = obj_data[start..end].trim().to_string();
            results.push((*obj_num, obj_content));
        }
    }

    results
}

/// Validation result for PDF structural checks
#[derive(Debug, Clone)]
pub struct PdfValidation {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub page_count: usize,
    pub object_count: usize,
}

/// Lazy PDF document that indexes stream objects without fully parsing all objects upfront.
///
/// This is useful for large PDFs where you only need to extract text or inspect a subset
/// of pages. Only stream data byte ranges are indexed during construction; dictionaries,
/// arrays, and other objects are not materialized.
#[derive(Debug, Clone)]
pub struct LazyPdfDocument {
    pub version: String,
    pub catalog: u32,
    data: Vec<u8>,
    /// Object ID -> (data_start, data_end) byte range of stream payload
    stream_objects: HashMap<u32, (usize, usize)>,
}

impl LazyPdfDocument {
    /// Create a lazy document from raw PDF bytes without parsing all objects.
    pub fn load_from_bytes(data: &[u8]) -> Result<Self> {
        let content = String::from_utf8_lossy(data);
        let mut version = "1.4".to_string();
        if let Some(header) = content.lines().next()
            && header.starts_with("%PDF-") {
                version = header[5..].to_string();
            }

        let catalog = {
            let root_re = regex::Regex::new(r"/Root\s+(\d+)\s+\d+\s+R").unwrap();
            if let Some(caps) = root_re.captures(&content) {
                caps[1].parse::<u32>().unwrap_or(0)
            } else {
                0
            }
        };

        let stream_objects = Self::find_stream_object_offsets(data);

        Ok(LazyPdfDocument {
            version,
            catalog,
            data: data.to_vec(),
            stream_objects,
        })
    }

    pub fn load_from_file(filename: &str) -> Result<Self> {
        let mut file = File::open(filename)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        Self::load_from_bytes(&buffer)
    }

    /// Scan for objects containing streams and record their ID -> byte range mapping.
    fn find_stream_object_offsets(data: &[u8]) -> HashMap<u32, (usize, usize)> {
        let content = String::from_utf8_lossy(data);
        let obj_re = regex::Regex::new(r"(\d+)\s+(\d+)\s+obj\b").unwrap();
        let mut result = HashMap::new();

        for caps in obj_re.captures_iter(&content) {
            let id = caps[1].parse::<u32>().unwrap_or(0);
            let obj_start = caps.get(0).unwrap().end();

            if let Some(endobj_pos) = content[obj_start..].find("endobj") {
                let obj_end = obj_start + endobj_pos;
                let obj_slice = &content[obj_start..obj_end];

                if let Some(stream_pos) = obj_slice.find("stream") {
                    let mut data_start_rel = stream_pos + "stream".len();
                    // Skip \r and/or \n after "stream"
                    while data_start_rel < obj_slice.len() {
                        let b = obj_slice.as_bytes()[data_start_rel];
                        if b == b'\r' || b == b'\n' {
                            data_start_rel += 1;
                        } else {
                            break;
                        }
                    }

                    if let Some(endstream_pos) = obj_slice[data_start_rel..].find("endstream") {
                        let data_end_rel = data_start_rel + endstream_pos;
                        // Trim trailing whitespace before endstream
                        let mut final_end = data_end_rel;
                        while final_end > data_start_rel {
                            let b = obj_slice.as_bytes()[final_end - 1];
                            if b == b'\r' || b == b'\n' {
                                final_end -= 1;
                            } else {
                                break;
                            }
                        }
                        let abs_start = obj_start + data_start_rel;
                        let abs_end = obj_start + final_end;
                        result.insert(id, (abs_start, abs_end));
                    }
                }
            }
        }

        result
    }

    /// Extract text by lazily decompressing and parsing only content stream objects.
    pub fn get_text(&self) -> Result<String> {
        let mut text = String::new();
        let tj_re = regex::Regex::new(r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)\s*Tj").unwrap();
        let tj_hex_re = regex::Regex::new(r"<([0-9a-fA-F\s]+)>\s*Tj").unwrap();
        let tj_array_re = regex::Regex::new(r"\[((?:[^\]]*?))\]\s*TJ").unwrap();
        let tj_str_re = regex::Regex::new(r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)").unwrap();
        let tj_hex_str_re = regex::Regex::new(r"<([0-9a-fA-F\s]+)>").unwrap();

        let mut ids: Vec<u32> = self.stream_objects.keys().copied().collect();
        ids.sort();

        for id in ids {
            if let Some(&(start, end)) = self.stream_objects.get(&id) {
                let data = &self.data[start..end];
                let processed = decompress_stream(data);
                let content = String::from_utf8_lossy(&processed);

                // Only process streams that contain text operators
                if content.contains("Tj") || content.contains("TJ") || content.contains("BT") {
                    for cap in tj_re.captures_iter(&content) {
                        if let Some(m) = cap.get(1) {
                            text.push_str(m.as_str());
                            text.push(' ');
                        }
                    }

                    for cap in tj_hex_re.captures_iter(&content) {
                        if let Some(m) = cap.get(1)
                            && let Some(bytes) = Self::decode_hex(m.as_str())
                                && let Ok(s) = String::from_utf8(bytes) {
                                    text.push_str(&s);
                                    text.push(' ');
                                }
                    }

                    for cap in tj_array_re.captures_iter(&content) {
                        if let Some(m) = cap.get(1) {
                            for inner in tj_str_re.captures_iter(m.as_str()) {
                                if let Some(inner_m) = inner.get(1) {
                                    text.push_str(inner_m.as_str());
                                }
                            }
                            for inner in tj_hex_str_re.captures_iter(m.as_str()) {
                                if let Some(inner_m) = inner.get(1)
                                    && let Some(bytes) = Self::decode_hex(inner_m.as_str())
                                        && let Ok(s) = String::from_utf8(bytes) {
                                            text.push_str(&s);
                                        }
                            }
                        }
                    }
                }
            }
        }

        Ok(text.trim().to_string())
    }

    fn decode_hex(s: &str) -> Option<Vec<u8>> {
        let cleaned: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if !cleaned.len().is_multiple_of(2) {
            return None;
        }
        let mut bytes = Vec::with_capacity(cleaned.len() / 2);
        for chunk in cleaned.as_bytes().chunks(2) {
            let hex = std::str::from_utf8(chunk).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
        }
        Some(bytes)
    }

    pub fn stream_object_count(&self) -> usize {
        self.stream_objects.len()
    }
}

/// Validate a PDF file's structural integrity
pub fn validate_pdf(filename: &str) -> Result<PdfValidation> {
    let mut file = File::open(filename)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(validate_pdf_bytes(&buffer))
}

/// Validate PDF bytes for structural integrity (library API — no filesystem needed)
pub fn validate_pdf_bytes(data: &[u8]) -> PdfValidation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let content = String::from_utf8_lossy(data);

    // 1. Check PDF header
    if !content.starts_with("%PDF-") {
        errors.push("Missing PDF header (%PDF-x.x)".to_string());
    } else {
        let version_end = content.find('\n').unwrap_or(10).min(10);
        let version = &content[5..version_end];
        if !version.starts_with("1.") && !version.starts_with("2.") {
            warnings.push(format!("Unusual PDF version: {}", version));
        }
    }

    // 2. Check %%EOF marker
    let trimmed_end = content.trim_end();
    if !trimmed_end.ends_with("%%EOF") {
        errors.push("Missing %%EOF marker at end of file".to_string());
    }

    // 3. Check xref table or xref stream
    let has_xref = content.contains("\nxref\n") || content.contains("\nxref\r\n");
    let has_startxref = content.contains("startxref");
    if !has_xref {
        warnings.push("No traditional xref table found (may use xref stream)".to_string());
    }
    if !has_startxref {
        errors.push("Missing startxref pointer".to_string());
    }

    // 4. Check trailer
    let has_trailer = content.contains("trailer");
    if !has_trailer && has_xref {
        errors.push("Missing trailer dictionary".to_string());
    }

    // 5. Check for Catalog
    let has_catalog = content.contains("/Type /Catalog");
    if !has_catalog {
        errors.push("Missing document catalog (/Type /Catalog)".to_string());
    }

    // 6. Check for Pages
    let has_pages = content.contains("/Type /Pages");
    if !has_pages {
        errors.push("Missing pages tree (/Type /Pages)".to_string());
    }

    // 7. Count page objects (/Type /Page but NOT /Type /Pages)
    let page_re = regex::Regex::new(r"/Type\s+/Page[^s]").unwrap();
    let page_re_eol = regex::Regex::new(r"/Type\s+/Page\s*\n").unwrap();
    let actual_pages = page_re.find_iter(&content).count() + page_re_eol.find_iter(&content).count();
    if actual_pages == 0 {
        errors.push("No page objects found (/Type /Page)".to_string());
    }

    // 8. Count objects
    let obj_re = regex::Regex::new(r"\d+\s+\d+\s+obj\b").unwrap();
    let object_count = obj_re.find_iter(&content).count();
    if object_count == 0 {
        errors.push("No PDF objects found".to_string());
    }

    // 9. Check object/endobj pairing
    let endobj_count = content.matches("endobj").count();
    if object_count != endobj_count {
        warnings.push(format!(
            "Object/endobj mismatch: {} obj vs {} endobj",
            object_count, endobj_count
        ));
    }

    // 10. Check stream/endstream pairing
    let stream_count = content.matches("\nstream\n").count()
        + content.matches("\nstream\r\n").count();
    let endstream_count = content.matches("endstream").count();
    if stream_count != endstream_count {
        warnings.push(format!(
            "Stream/endstream mismatch: {} stream vs {} endstream",
            stream_count, endstream_count
        ));
    }

    // 11. Check /Root reference in trailer
    if has_trailer {
        let root_re = regex::Regex::new(r"/Root\s+\d+\s+\d+\s+R").unwrap();
        if !root_re.is_match(&content) {
            errors.push("Trailer missing /Root reference".to_string());
        }
    }

    let valid = errors.is_empty();

    PdfValidation {
        valid,
        errors,
        warnings,
        page_count: actual_pages,
        object_count,
    }
}

/// Validation result for PDF/A compliance checks
#[derive(Debug, Clone)]
pub struct PdfAValidation {
    pub compliant: bool,
    pub level: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub embedded_fonts: bool,
    pub has_xmp: bool,
    pub has_encryption: bool,
}

/// Validate PDF bytes for PDF/A-1b compliance (basic level).
///
/// Checks the most important PDF/A-1b requirements that can be
/// detected with structural analysis (no full content stream parsing):
///
/// - **No encryption** — /Encrypt must not be present
/// - **No JavaScript** — /JS or /JavaScript actions must not be present
/// - **No external streams** — /F references in streams must not be present
/// - **Embedded fonts** — all /Font descriptors must reference a font file
/// - **XMP metadata** — catalog should contain /Metadata reference
pub fn validate_pdf_a_bytes(data: &[u8]) -> PdfAValidation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let content = String::from_utf8_lossy(data);

    // 1. No encryption
    let has_encryption = content.contains("/Encrypt") || content.contains("\nEncrypt");
    if has_encryption {
        errors.push("PDF contains encryption (not allowed in PDF/A)".to_string());
    }

    // 2. No JavaScript
    let has_js = content.contains("/JS") || content.contains("/JavaScript");
    if has_js {
        errors.push("PDF contains JavaScript (not allowed in PDF/A)".to_string());
    }

    // 3. No external file references in streams
    let has_external = content.contains("\n/F ") || content.contains("/F (");
    if has_external {
        errors.push("PDF contains external stream references (not allowed in PDF/A)".to_string());
    }

    // 4. Check for embedded fonts
    // Count font descriptors and font files; every descriptor should have a file
    let font_desc_count = content.matches("/Type /FontDescriptor").count();
    let font_file_count = content.matches("/FontFile").count()
        + content.matches("/FontFile2").count()
        + content.matches("/FontFile3").count();
    let embedded_fonts = font_desc_count == 0 || font_file_count >= font_desc_count;
    if !embedded_fonts {
        errors.push(format!(
            "Fonts not fully embedded: {} descriptors vs {} font files",
            font_desc_count, font_file_count
        ));
    }

    // 5. Check for XMP metadata
    let has_xmp = content.contains("/Type /Metadata") || content.contains("/Metadata ");
    if !has_xmp {
        warnings.push("No XMP metadata stream found (recommended for PDF/A)".to_string());
    }

    // 6. No transparency (PDF/A-1 specific)
    let has_transparency = content.contains("/CA ") || content.contains("/ca ");
    if has_transparency {
        warnings.push("Possible transparency group detected (not allowed in PDF/A-1)".to_string());
    }

    // 7. No launch actions
    if content.contains("/S /Launch") {
        errors.push("PDF contains launch actions (not allowed in PDF/A)".to_string());
    }

    let compliant = errors.is_empty();

    PdfAValidation {
        compliant,
        level: "PDF/A-1b".to_string(),
        errors,
        warnings,
        embedded_fonts,
        has_xmp,
        has_encryption,
    }
}

/// Validate a PDF file for PDF/A-1b compliance
pub fn validate_pdf_a(filename: &str) -> Result<PdfAValidation> {
    let mut file = File::open(filename)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(validate_pdf_a_bytes(&buffer))
}

/// Validate PDF bytes for PDF/A-3b compliance.
///
/// PDF/A-3 is identical to PDF/A-1b except embedded files are *required*
/// (one or more attachments must be present in /Names -> /EmbeddedFiles).
pub fn validate_pdf_a3_bytes(data: &[u8]) -> PdfAValidation {
    let mut result = validate_pdf_a_bytes(data);
    result.level = "PDF/A-3b".to_string();

    let content = String::from_utf8_lossy(data);

    // PDF/A-3 requires at least one embedded file
    let has_embedded_files = content.contains("/EmbeddedFiles")
        && content.contains("/Filespec")
        && content.contains("/EmbeddedFile");
    if !has_embedded_files {
        result.errors.push("PDF/A-3 requires at least one embedded file attachment".to_string());
    }

    // Remove the transparency warning that only applies to PDF/A-1
    result.warnings.retain(|w| !w.contains("PDF/A-1"));

    // Recompute compliance
    result.compliant = result.errors.is_empty();
    result
}

/// Validate a PDF file for PDF/A-3b compliance
pub fn validate_pdf_a3(filename: &str) -> Result<PdfAValidation> {
    let mut file = File::open(filename)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(validate_pdf_a3_bytes(&buffer))
}

/// Validation result for PDF/UA (accessibility) compliance
#[derive(Debug, Clone)]
pub struct PdfUaValidation {
    pub compliant: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub has_mark_info: bool,
    pub has_struct_tree: bool,
    pub has_lang: bool,
    pub has_title: bool,
    pub fonts_embedded: bool,
}

/// Validate PDF bytes for PDF/UA-1 compliance (basic structural checks).
///
/// Checks the most important PDF/UA requirements detectable
/// through structural analysis:
///
/// - **/MarkInfo** — catalog must contain `/MarkInfo << /Marked true >>`
/// - **/StructTreeRoot** — catalog must reference a structure tree
/// - **/Lang** — catalog or page must declare a language
/// - **Title** — document must have a title in Info or XMP
/// - **No encryption** — security handlers interfere with assistive tech
/// - **Embedded fonts** — all fonts must be embedded for text extraction
pub fn validate_pdf_ua_bytes(data: &[u8]) -> PdfUaValidation {
    let mut errors = Vec::new();
    let warnings = Vec::new();
    let content = String::from_utf8_lossy(data);

    // 1. MarkInfo / Marked must be true
    let has_mark_info = content.contains("/MarkInfo")
        && (content.contains("/Marked true") || content.contains("/Marked\ntrue"));
    if !has_mark_info {
        errors.push("Missing /MarkInfo << /Marked true >> (required for PDF/UA)".to_string());
    }

    // 2. StructTreeRoot must exist
    let has_struct_tree = content.contains("/StructTreeRoot");
    if !has_struct_tree {
        errors.push("Missing /StructTreeRoot (required for tagged PDF)".to_string());
    }

    // 3. Language must be declared
    let has_lang = content.contains("/Lang") || content.contains("/Lang ");
    if !has_lang {
        errors.push("Missing /Lang attribute (required for PDF/UA)".to_string());
    }

    // 4. Document title
    let has_title = content.contains("/Title") || content.contains("<dc:title>");
    if !has_title {
        errors.push("Missing document title (required for PDF/UA)".to_string());
    }

    // 5. No encryption
    let has_encryption = content.contains("/Encrypt") || content.contains("\nEncrypt");
    if has_encryption {
        errors.push("Encryption prevents screen reader access (not allowed in PDF/UA)".to_string());
    }

    // 6. Fonts must be embedded
    let font_desc_count = content.matches("/Type /FontDescriptor").count();
    let font_file_count = content.matches("/FontFile").count()
        + content.matches("/FontFile2").count()
        + content.matches("/FontFile3").count();
    let fonts_embedded = font_desc_count == 0 || font_file_count >= font_desc_count;
    if !fonts_embedded {
        errors.push("Fonts not fully embedded (required for text extraction in PDF/UA)".to_string());
    }

    // 7. No JavaScript (interferes with assistive technology)
    if content.contains("/JS") || content.contains("/JavaScript") {
        errors.push("JavaScript actions interfere with assistive technology".to_string());
    }

    let compliant = errors.is_empty();

    PdfUaValidation {
        compliant,
        errors,
        warnings,
        has_mark_info,
        has_struct_tree,
        has_lang,
        has_title,
        fonts_embedded,
    }
}

/// Validate a PDF file for PDF/UA compliance
pub fn validate_pdf_ua(filename: &str) -> Result<PdfUaValidation> {
    let mut file = File::open(filename)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(validate_pdf_ua_bytes(&buffer))
}

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
    let page_re = regex::Regex::new(r"/Type\s+/Page[^s]").unwrap();
    let pages_old = page_re.find_iter(&old_content).count();
    let pages_new = page_re.find_iter(&new_content).count();

    // Compute object-level changes
    let mut added_objects = Vec::new();
    let mut removed_objects = Vec::new();
    let mut modified_objects = Vec::new();

    for id in old_doc.objects.keys() {
        if !new_doc.objects.contains_key(id) {
            removed_objects.push(*id);
        } else if PdfDocument::object_content_key(&old_doc.objects[id]) != PdfDocument::object_content_key(&new_doc.objects[id]) {
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

    let has_embedded_files_old = old_content.contains("/EmbeddedFiles") && old_content.contains("/Filespec");
    let has_embedded_files_new = new_content.contains("/EmbeddedFiles") && new_content.contains("/Filespec");

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

pub fn extract_text(filename: &str) -> Result<String> {
    let doc = PdfDocument::load_from_file(filename)?;
    let text = doc.get_text()?;
    Ok(text)
}

pub fn unescape_pdf_string(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('(') => result.push('('),
                Some(')') => result.push(')'),
                Some('b') => result.push('\u{0008}'),
                Some('f') => result.push('\u{000C}'),
                Some(d) if d.is_ascii_digit() => {
                    let mut octal = String::new();
                    octal.push(d);
                    for _ in 0..2 {
                        if let Some(&next) = chars.peek() {
                            if next.is_ascii_digit() && ('0'..='7').contains(&next) {
                                octal.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    if let Ok(code) = u8::from_str_radix(&octal, 8) {
                        if code > 0 {
                            result.push(code as char);
                        }
                    } else {
                        result.push('\\');
                        result.push(d);
                    }
                }
                Some(other) => {
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub fn decode_pdf_hex_string(s: &str) -> String {
    let hex_str: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut bytes = Vec::new();
    
    for i in (0..hex_str.len()).step_by(2) {
        if i + 1 < hex_str.len() {
            let byte_str = &hex_str[i..i + 2];
            if let Ok(byte) = u8::from_str_radix(byte_str, 16) {
                bytes.push(byte);
            }
        } else if i < hex_str.len() {
            let byte_str = &hex_str[i..i + 1];
            if let Ok(byte) = u8::from_str_radix(&format!("{}0", byte_str), 16) {
                bytes.push(byte);
            }
        }
    }
    
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        decode_utf16be(&bytes[2..])
    } else {
        if let Some(decoded) = decode_unicode_glyph_id_bytes(&bytes) {
            return decoded;
        }
        String::from_utf8_lossy(&bytes).to_string()
    }
}

fn resolve_unicode_ttf_path_for_extraction() -> Option<String> {
    if let Ok(path) = std::env::var("PDFRS_UNICODE_FONT_PATH")
        && !path.trim().is_empty() && Path::new(&path).exists() {
            return Some(path);
        }

    let candidates = [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
    ];

    candidates
        .iter()
        .find(|p| Path::new(p).exists())
        .map(|p| (*p).to_string())
}

fn build_unicode_gid_reverse_map() -> Option<HashMap<u16, char>> {
    let font_path = resolve_unicode_ttf_path_for_extraction()?;
    let font_bytes = fs::read(font_path).ok()?;
    let face = ttf_parser::Face::parse(&font_bytes, 0).ok()?;

    let mut reverse_map = HashMap::new();
    for cp in 0u32..=0x10FFFF {
        let Some(ch) = char::from_u32(cp) else {
            continue;
        };
        if let Some(glyph) = face.glyph_index(ch) {
            reverse_map.entry(glyph.0).or_insert(ch);
        }
    }

    Some(reverse_map)
}

fn decode_unicode_glyph_id_bytes(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return None;
    }

    static GID_REVERSE_MAP: OnceLock<Option<HashMap<u16, char>>> = OnceLock::new();
    let gid_map = GID_REVERSE_MAP
        .get_or_init(build_unicode_gid_reverse_map)
        .as_ref()?;

    let mut out = String::with_capacity(bytes.len() / 2);
    let mut known_count = 0usize;
    let total = bytes.len() / 2;

    for chunk in bytes.chunks_exact(2) {
        let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
        if let Some(ch) = gid_map.get(&gid) {
            out.push(*ch);
            known_count += 1;
        } else if gid == 0 {
            out.push(' ');
        } else {
            out.push('\u{FFFD}');
        }
    }

    // Require a strong hit-rate to avoid mis-decoding arbitrary hex payloads.
    if known_count == 0 || known_count * 10 < total * 6 {
        return None;
    }

    Some(out)
}

fn decode_utf16be(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;
    
    while i + 1 < bytes.len() {
        let high = (bytes[i] as u16) << 8 | (bytes[i + 1] as u16);
        i += 2;
        
        if (0xD800..=0xDBFF).contains(&high) && i + 1 < bytes.len() {
            let low = (bytes[i] as u16) << 8 | (bytes[i + 1] as u16);
            if (0xDC00..=0xDFFF).contains(&low) {
                i += 2;
                let codepoint = 0x10000u32 + ((high as u32 - 0xD800) << 10) + (low as u32 - 0xDC00);
                if let Some(ch) = char::from_u32(codepoint) {
                    result.push(ch);
                }
                continue;
            }
        }
        
        if let Some(ch) = char::from_u32(high as u32) {
            result.push(ch);
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unescape_pdf_string() {
        assert_eq!(unescape_pdf_string(r"hello"), "hello");
        assert_eq!(unescape_pdf_string(r"hello\nworld"), "hello\nworld");
        assert_eq!(unescape_pdf_string(r"a\(b\)c"), "a(b)c");
        assert_eq!(unescape_pdf_string(r"back\\slash"), "back\\slash");
        assert_eq!(unescape_pdf_string(r"tab\there"), "tab\there");
        assert_eq!(unescape_pdf_string(r"form\ffeed"), "form\u{000C}feed");
        assert_eq!(unescape_pdf_string(r"back\bspace"), "back\u{0008}space");
    }

    #[test]
    fn test_unescape_octal_sequences() {
        assert_eq!(unescape_pdf_string(r"\101"), "A");
        assert_eq!(unescape_pdf_string(r"\101\102\103"), "ABC");
        assert_eq!(unescape_pdf_string(r"\60"), "0");
        assert_eq!(unescape_pdf_string(r"\141\142\143"), "abc");
        assert_eq!(unescape_pdf_string(r"Hello\40World"), "Hello World");
    }

    #[test]
    fn test_decode_hex_string_basic() {
        assert_eq!(decode_pdf_hex_string("48656C6C6F"), "Hello");
        assert_eq!(decode_pdf_hex_string("576F726C64"), "World");
        assert_eq!(decode_pdf_hex_string("414243"), "ABC");
        assert_eq!(decode_pdf_hex_string("48 65 6C 6C 6F"), "Hello");
    }

    #[test]
    fn test_decode_hex_string_utf16be() {
        assert_eq!(decode_pdf_hex_string("FEFF00480065006C006C006F"), "Hello");
        assert_eq!(decode_pdf_hex_string("FEFF4F60597D"), "你好");
        assert_eq!(decode_pdf_hex_string("FEFF0041004200430044"), "ABCD");
    }

    #[test]
    fn test_decode_hex_string_unicode_symbols() {
        assert_eq!(decode_pdf_hex_string("FEFF03B103B203B3"), "αβγ");
        assert_eq!(decode_pdf_hex_string("FEFF221E2211222B"), "∞∑∫");
    }

    #[test]
    fn test_decode_hex_string_unicode_glyph_ids_roundtrip() {
        let Some(path) = resolve_unicode_ttf_path_for_extraction() else {
            return;
        };
        let Ok(bytes) = fs::read(path) else {
            return;
        };
        let Ok(face) = ttf_parser::Face::parse(&bytes, 0) else {
            return;
        };

        let sample = "Unicode test: 你好 Γεια ∑";
        let mut encoded = String::new();
        for ch in sample.chars() {
            let Some(gid) = face.glyph_index(ch) else {
                return;
            };
            encoded.push_str(&format!("{:04X}", gid.0));
        }

        assert_eq!(decode_pdf_hex_string(&encoded), sample);
    }

    #[test]
    fn test_decode_utf16be_surrogate_pairs() {
        let bytes = vec![0xD8, 0x3D, 0xDE, 0x00];
        assert_eq!(decode_utf16be(&bytes), "😀");
        
        let bytes2 = vec![0xD8, 0x3D, 0xDE, 0x01];
        assert_eq!(decode_utf16be(&bytes2), "😁");
    }

    #[test]
    fn test_winansi_decode() {
        assert_eq!(winansi_decode(0x41), 'A');
        assert_eq!(winansi_decode(0x80), '\u{20AC}'); // Euro
        assert_eq!(winansi_decode(0x95), '\u{2022}'); // Bullet
        assert_eq!(winansi_decode(0x96), '\u{2013}'); // En dash
        assert_eq!(winansi_decode(0x97), '\u{2014}'); // Em dash
    }

    #[test]
    fn test_macroman_decode() {
        assert_eq!(macroman_decode(0x41), 'A');
        assert_eq!(macroman_decode(0x80), '\u{00C4}'); // Ä
        assert_eq!(macroman_decode(0x8A), '\u{00E4}'); // ä (index 10 in high table)
    }

    #[test]
    fn test_decode_with_encoding() {
        let data = b"Hello";
        assert_eq!(decode_with_encoding(data, "WinAnsiEncoding"), "Hello");
        assert_eq!(decode_with_encoding(data, "MacRomanEncoding"), "Hello");
        assert_eq!(decode_with_encoding(data, "StandardEncoding"), "Hello");
    }

    #[test]
    fn test_parse_dict_entries() {
        let raw = "<< /Type /Page /Length 42 >>";
        let dict = parse_dict_entries(raw);
        assert!(dict.contains_key("Type"));
        assert!(dict.contains_key("Length"));
    }

    #[test]
    fn test_text_position_tracker() {
        let mut tracker = TextPositionTracker::new();
        assert!(!tracker.moved_to_new_line(720.0)); // first call, no previous
        assert!(!tracker.moved_to_new_line(720.0)); // same Y
        assert!(tracker.moved_to_new_line(700.0));  // moved 20 units
        assert!(!tracker.moved_to_new_line(700.0)); // same Y again
    }

    #[test]
    fn test_decompress_stream_passthrough() {
        let data = b"BT /F1 12 Tf (Hello) Tj ET";
        let result = decompress_stream(data);
        assert_eq!(result, data);
    }

    #[test]
    fn test_read_xref_field() {
        // 1-byte field
        assert_eq!(read_xref_field(&[0x01], 0, 1), 1);
        assert_eq!(read_xref_field(&[0xFF], 0, 1), 255);

        // 2-byte field (big-endian)
        assert_eq!(read_xref_field(&[0x01, 0x00], 0, 2), 256);
        assert_eq!(read_xref_field(&[0x00, 0x2A], 0, 2), 42);

        // 3-byte field
        assert_eq!(read_xref_field(&[0x01, 0x00, 0x00], 0, 3), 65536);

        // 0-width field
        assert_eq!(read_xref_field(&[0xFF], 0, 0), 0);
    }

    #[test]
    fn test_parse_xref_stream_basic() {
        // W = [1, 2, 1], size = 3
        // Entry 0: type=0, offset=0x0000, gen=0xFF (free)
        // Entry 1: type=1, offset=0x0100, gen=0x00 (normal at offset 256)
        // Entry 2: type=2, offset=0x0005, gen=0x02 (compressed in obj 5, index 2)
        let data: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0xFF, // entry 0: type=0, field2=0, field3=255
            0x01, 0x01, 0x00, 0x00, // entry 1: type=1, field2=256, field3=0
            0x02, 0x00, 0x05, 0x02, // entry 2: type=2, field2=5, field3=2
        ];
        let w = vec![1, 2, 1];
        let entries = parse_xref_stream(&data, &w, 3);

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0], (0, 0, 255));
        assert_eq!(entries[1], (1, 256, 0));
        assert_eq!(entries[2], (2, 5, 2));
    }

    #[test]
    fn test_parse_xref_stream_empty() {
        let entries = parse_xref_stream(&[], &[1, 2, 1], 0);
        assert!(entries.is_empty());

        let entries = parse_xref_stream(&[0x01], &[], 1);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_object_stream() {
        // Object stream with 2 objects:
        // Header: "10 0 20 14 " (obj 10 at offset 0, obj 20 at offset 14)
        // First = 10 (offset where object data starts)
        // Data after first: "<< /Type /Page >>null"
        let stream = b"10 0 20 14 << /Type /Page >>null";
        let first = 11; // "10 0 20 14 " is 11 bytes
        let results = parse_object_stream(stream, 2, first);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 10); // obj num
        assert!(results[0].1.contains("/Type"));
        assert_eq!(results[1].0, 20); // obj num
    }

    #[test]
    fn test_parse_object_stream_empty() {
        let results = parse_object_stream(b"", 0, 0);
        assert!(results.is_empty());

        // first beyond data length
        let results = parse_object_stream(b"10 0 ", 1, 100);
        assert!(results.is_empty());
    }

    #[test]
    fn test_validate_pdf_bytes_valid() {
        // Generate a valid PDF via the library
        let elements = vec![
            crate::elements::Element::Heading { level: 1, text: "Test Title".into() },
            crate::elements::Element::Paragraph { text: "Hello world paragraph.".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let result = validate_pdf_bytes(&pdf_bytes);
        assert!(result.valid, "Generated PDF should be valid. Errors: {:?}", result.errors);
        assert!(result.page_count >= 1, "Should have at least 1 page");
        assert!(result.object_count > 0, "Should have objects");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_pdf_bytes_invalid_header() {
        let result = validate_pdf_bytes(b"NOT A PDF FILE");
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("Missing PDF header")));
    }

    #[test]
    fn test_validate_pdf_bytes_empty() {
        let result = validate_pdf_bytes(b"");
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("Missing PDF header")));
    }

    #[test]
    fn test_validate_pdf_bytes_missing_eof() {
        let result = validate_pdf_bytes(b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n");
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("%%EOF")));
    }

    #[test]
    fn test_roundtrip_generate_validate_parse() {
        // Round-trip: elements → PDF bytes → validate → parse → extract text → verify
        let elements = vec![
            crate::elements::Element::Heading { level: 1, text: "Roundtrip Title".into() },
            crate::elements::Element::Paragraph { text: "This is roundtrip content.".into() },
            crate::elements::Element::UnorderedListItem { text: "Item one".into(), depth: 0 },
            crate::elements::Element::UnorderedListItem { text: "Item two".into(), depth: 0 },
            crate::elements::Element::CodeBlock { language: "rust".into(), code: "fn main() {}".into() },
            crate::elements::Element::BlockQuote { text: "A quote".into(), depth: 1 },
            crate::elements::Element::Link { text: "Example".into(), url: "https://example.com".into() },
            crate::elements::Element::Image { alt: "Logo".into(), path: "logo.png".into() },
            crate::elements::Element::Footnote { label: "1".into(), text: "A footnote.".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        // 1. Validate structure
        let validation = validate_pdf_bytes(&pdf_bytes);
        assert!(validation.valid, "PDF should be valid. Errors: {:?}", validation.errors);
        assert!(validation.page_count >= 1);

        // 2. Parse back and extract text
        let content = String::from_utf8_lossy(&pdf_bytes);
        // Check key content strings are present in the raw PDF
        assert!(content.contains("Roundtrip Title"), "Title not found in PDF");
        assert!(content.contains("roundtrip content"), "Paragraph not found in PDF");
        assert!(content.contains("Item one"), "List item not found in PDF");
        assert!(content.contains("fn") && content.contains("main"), "Code block not found in PDF");
        assert!(content.contains("quote"), "Blockquote not found in PDF");
        assert!(content.contains("Example"), "Link text not found in PDF");
        assert!(content.contains("example.com"), "Link URL not found in PDF");
        assert!(content.contains("Logo"), "Image alt not found in PDF");
        assert!(content.contains("footnote"), "Footnote not found in PDF");
    }

    #[test]
    fn test_roundtrip_all_element_types() {
        // Comprehensive round-trip: every element type → PDF → validate → verify text
        let elements = vec![
            crate::elements::Element::Heading { level: 1, text: "H1 Title".into() },
            crate::elements::Element::Heading { level: 2, text: "H2 Subtitle".into() },
            crate::elements::Element::Heading { level: 3, text: "H3 Section".into() },
            crate::elements::Element::Paragraph { text: "Normal paragraph text here.".into() },
            crate::elements::Element::EmptyLine,
            crate::elements::Element::UnorderedListItem { text: "Bullet item".into(), depth: 0 },
            crate::elements::Element::OrderedListItem { number: 1, text: "Numbered item".into(), depth: 0 },
            crate::elements::Element::TaskListItem { checked: true, text: "Done task".into() },
            crate::elements::Element::TaskListItem { checked: false, text: "Todo task".into() },
            crate::elements::Element::CodeBlock { language: "python".into(), code: "print('hello')".into() },
            crate::elements::Element::InlineCode { code: "let x = 42".into() },
            crate::elements::Element::TableRow {
                cells: vec!["Name".into(), "Age".into()],
                is_separator: false,
                alignments: vec![crate::elements::TableAlignment::Left, crate::elements::TableAlignment::Left],
            },
            crate::elements::Element::BlockQuote { text: "Wise words".into(), depth: 1 },
            crate::elements::Element::DefinitionItem { term: "Rust".into(), definition: "A language".into() },
            crate::elements::Element::Footnote { label: "fn1".into(), text: "See reference".into() },
            crate::elements::Element::Link { text: "Google".into(), url: "https://google.com".into() },
            crate::elements::Element::Image { alt: "Photo".into(), path: "photo.jpg".into() },
            crate::elements::Element::StyledText { text: "Bold text".into(), bold: true, italic: false },
            crate::elements::Element::HorizontalRule,
            crate::elements::Element::PageBreak,
            crate::elements::Element::Paragraph { text: "After page break.".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        // Validate
        let validation = validate_pdf_bytes(&pdf_bytes);
        assert!(validation.valid, "PDF with all elements should be valid. Errors: {:?}", validation.errors);
        assert!(validation.page_count >= 2, "PageBreak should create at least 2 pages, got {}", validation.page_count);

        // Verify content
        let content = String::from_utf8_lossy(&pdf_bytes);
        let expected_strings = vec![
            "H1 Title", "H2 Subtitle", "H3 Section",
            "Normal paragraph", "Bullet item", "Numbered item",
            "Done task", "Todo task", "print", "let", "x = 42",
            "Name", "Age", "Wise words", "Rust", "A language",
            "See reference", "Google", "google.com",
            "Photo", "photo.jpg", "Bold text", "After page break",
        ];
        for s in &expected_strings {
            assert!(content.contains(s), "Expected '{}' in PDF content", s);
        }
    }

    #[test]
    fn test_roundtrip_landscape() {
        let elements = vec![
            crate::elements::Element::Heading { level: 1, text: "Landscape Doc".into() },
            crate::elements::Element::Paragraph { text: "Wide content.".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::landscape();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let validation = validate_pdf_bytes(&pdf_bytes);
        assert!(validation.valid, "Landscape PDF should be valid. Errors: {:?}", validation.errors);

        // Check landscape dimensions (792 x 612)
        let content = String::from_utf8_lossy(&pdf_bytes);
        assert!(content.contains("792"), "Landscape width should be 792");
        assert!(content.contains("612"), "Landscape height should be 612");
    }

    #[test]
    fn test_load_from_bytes_roundtrip() {
        let elements = vec![
            crate::elements::Element::Heading { level: 1, text: "Roundtrip".into() },
            crate::elements::Element::Paragraph { text: "Testing load_from_bytes.".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        // Parse from bytes
        let doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        assert!(!doc.objects.is_empty());

        // Serialize back to bytes
        let roundtrip_bytes = doc.to_bytes();
        assert!(!roundtrip_bytes.is_empty());

        // Re-parse and verify text is intact
        let doc2 = PdfDocument::load_from_bytes(&roundtrip_bytes).unwrap();
        let text = doc2.get_text().unwrap();
        assert!(text.contains("Roundtrip"), "Text lost after roundtrip: {}", text);
        assert!(text.contains("Testing load_from_bytes."), "Text lost after roundtrip: {}", text);
    }

    #[test]
    fn test_validate_pdf_a_generated_pdf() {
        let elements = vec![
            crate::elements::Element::Heading { level: 1, text: "PDF/A Test".into() },
            crate::elements::Element::Paragraph { text: "Testing PDF/A validation.".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let result = validate_pdf_a_bytes(&pdf_bytes);
        // Generated PDFs use Base-14 fonts without embedding, so they won't be fully PDF/A compliant
        // but should have no encryption, no JS, no external references
        assert!(!result.has_encryption, "Generated PDF should not have encryption");
        assert!(!result.errors.iter().any(|e| e.contains("JavaScript")), "No JS expected");
        assert!(!result.errors.iter().any(|e| e.contains("external")), "No external refs expected");
    }

    #[test]
    fn test_deduplicate_objects() {
        let mut doc = PdfDocument::new();

        // Insert two identical objects
        doc.objects.insert(1, PdfObject::String("shared_content".to_string()));
        doc.objects.insert(2, PdfObject::String("shared_content".to_string()));

        // Insert a dictionary that references object 2
        let mut dict = HashMap::new();
        dict.insert("Ref".to_string(), PdfValue::Object(PdfObject::String("2 0 R".to_string())));
        doc.objects.insert(3, PdfObject::Dictionary(dict));
        doc.catalog = 3;

        assert_eq!(doc.objects.len(), 3, "Should start with 3 objects");

        doc.deduplicate_objects();

        // Object 2 (duplicate) should be removed; object 1 (canonical) kept
        assert_eq!(doc.objects.len(), 2, "Should remove one duplicate");
        assert!(doc.objects.contains_key(&1), "Canonical object 1 should remain");
        assert!(!doc.objects.contains_key(&2), "Duplicate object 2 should be removed");
        assert!(doc.objects.contains_key(&3), "Referencing object 3 should remain");

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
    fn test_lazy_pdf_document_text_extraction() {
        let elements = vec![
            crate::elements::Element::Heading { level: 1, text: "Lazy Test".into() },
            crate::elements::Element::Paragraph { text: "Testing lazy text extraction.".into() },
            crate::elements::Element::Paragraph { text: "Second paragraph for good measure.".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        // Lazy document should extract same text as full document
        let lazy_doc = LazyPdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        let lazy_text = lazy_doc.get_text().unwrap();

        let full_doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        let full_text = full_doc.get_text().unwrap();

        assert!(
            lazy_text.contains("Lazy Test"),
            "Lazy text should contain heading: {}", lazy_text
        );
        assert!(
            lazy_text.contains("Testing lazy text extraction."),
            "Lazy text should contain paragraph: {}", lazy_text
        );
        assert!(
            lazy_text.contains("Second paragraph"),
            "Lazy text should contain second paragraph: {}", lazy_text
        );

        // Lazy document should have fewer/no non-stream objects materialized
        // but text content should be equivalent
        assert!(
            !lazy_text.is_empty(),
            "Lazy text extraction should produce non-empty output"
        );
    }

    #[test]
    fn test_embed_file_attachment() {
        let elements = vec![
            crate::elements::Element::Paragraph { text: "Document with attachment".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

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
        assert!(doc.objects.contains_key(&fs_id), "File spec object should exist");

        // Verify the embedded file stream exists (should be fs_id - 1)
        let ef_id = fs_id - 1;
        assert!(doc.objects.contains_key(&ef_id), "Embedded file stream object should exist");

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
        assert!(!output_bytes.is_empty(), "PDF with attachment should serialize");

        // Verify /EmbeddedFile appears in output
        let content = String::from_utf8_lossy(&output_bytes);
        assert!(content.contains("/EmbeddedFile"), "Output should contain /EmbeddedFile type");
        assert!(content.contains("/Filespec"), "Output should contain /Filespec type");
        assert!(content.contains("test.txt"), "Output should contain attachment filename");
    }

    #[test]
    fn test_validate_pdf_a3_fails_without_embedded_files() {
        let elements = vec![
            crate::elements::Element::Paragraph { text: "No attachments".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let result = validate_pdf_a3_bytes(&pdf_bytes);
        assert!(
            result.errors.iter().any(|e| e.contains("embedded file")),
            "PDF/A-3 should fail without embedded files: {:?}",
            result.errors
        );
        assert!(!result.compliant, "Should not be PDF/A-3 compliant without attachments");
    }

    #[test]
    fn test_validate_pdf_a3_passes_with_embedded_files() {
        let elements = vec![
            crate::elements::Element::Paragraph { text: "With attachment".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let mut doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        doc.embed_file("data.csv", b"a,b,c\n1,2,3").unwrap();
        let output_bytes = doc.to_bytes();

        let result = validate_pdf_a3_bytes(&output_bytes);
        // Still may fail on other PDF/A checks (fonts, XMP) but should NOT fail on embedded files
        assert!(
            !result.errors.iter().any(|e| e.contains("embedded file")),
            "PDF/A-3 should not complain about embedded files when present: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_validate_pdf_ua_detects_missing_accessibility() {
        let elements = vec![
            crate::elements::Element::Paragraph { text: "Untagged doc".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let result = validate_pdf_ua_bytes(&pdf_bytes);
        // Our generated PDFs don't have MarkInfo, StructTreeRoot, Lang, or Title yet
        assert!(!result.compliant, "Untagged PDF should not be PDF/UA compliant");
        assert!(!result.has_mark_info, "Should detect missing MarkInfo");
        assert!(!result.has_struct_tree, "Should detect missing StructTreeRoot");
        assert!(!result.has_lang, "Should detect missing Lang");
        assert!(!result.has_title, "Should detect missing Title");
    }

    #[test]
    fn test_sanitize_removes_dangerous_objects() {
        let elements = vec![
            crate::elements::Element::Paragraph { text: "Safe document".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let mut doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        let original_count = doc.objects.len();

        // Inject a fake JavaScript object
        let mut js_dict = HashMap::new();
        js_dict.insert("JS".to_string(), PdfValue::Object(PdfObject::String("app.alert('xss')".to_string())));
        doc.objects.insert(999, PdfObject::Dictionary(js_dict));

        // Inject a fake launch action
        let mut launch_dict = HashMap::new();
        launch_dict.insert("S".to_string(), PdfValue::Object(PdfObject::String("/Launch".to_string())));
        launch_dict.insert("F".to_string(), PdfValue::Object(PdfObject::String("(malware.exe)".to_string())));
        doc.objects.insert(998, PdfObject::Dictionary(launch_dict));

        // Add OpenAction to catalog
        if let Some(PdfObject::Dictionary(catalog_dict)) = doc.objects.get_mut(&doc.catalog) {
            catalog_dict.insert("OpenAction".to_string(), PdfValue::Object(PdfObject::String("999 0 R".to_string())));
        }

        assert_eq!(doc.objects.len(), original_count + 2, "Should have injected 2 dangerous objects");

        // Sanitize
        doc.sanitize();

        // JS object should be removed entirely
        assert!(!doc.objects.contains_key(&999), "JavaScript object should be removed");

        // Launch action should be removed entirely
        assert!(!doc.objects.contains_key(&998), "Launch action object should be removed");

        // Catalog should no longer have OpenAction
        if let Some(PdfObject::Dictionary(catalog_dict)) = doc.objects.get(&doc.catalog) {
            assert!(!catalog_dict.contains_key("OpenAction"), "OpenAction should be stripped from catalog");
        } else {
            panic!("Catalog should remain a dictionary");
        }

        // Safe objects should still be present
        assert_eq!(doc.objects.len(), original_count, "Only dangerous objects should be removed");

        // Verify PDF still serializes correctly
        let output_bytes = doc.to_bytes();
        assert!(!output_bytes.is_empty(), "Sanitized PDF should still serialize");
        let content = String::from_utf8_lossy(&output_bytes);
        assert!(!content.contains("app.alert"), "JS payload should not remain in output");
    }

    #[test]
    fn test_diff_pdf_bytes_detects_changes() {
        let elements_old = vec![
            crate::elements::Element::Paragraph { text: "First version".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let old_bytes = crate::pdf_generator::generate_pdf_bytes(&elements_old, "Helvetica", 12.0, layout).unwrap();

        let elements_new = vec![
            crate::elements::Element::Paragraph { text: "Second version with more content".into() },
            crate::elements::Element::Paragraph { text: "Extra paragraph".into() },
        ];
        let new_bytes = crate::pdf_generator::generate_pdf_bytes(&elements_new, "Helvetica", 12.0, layout).unwrap();

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
        let elements = vec![
            crate::elements::Element::Paragraph { text: "Same content".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let diff = diff_pdf_bytes(&bytes, &bytes).unwrap();

        // Identical PDFs should have 100% text similarity
        assert_eq!(diff.text_similarity, 1.0, "Identical PDFs should have 100% text similarity");

        // No added, removed, or modified objects
        assert!(diff.added_objects.is_empty(), "Identical PDFs should have no added objects");
        assert!(diff.removed_objects.is_empty(), "Identical PDFs should have no removed objects");
        assert!(diff.modified_objects.is_empty(), "Identical PDFs should have no modified objects");
    }

    #[test]
    fn test_repl_like_workflow() {
        // Simulate a REPL session: create -> load -> modify -> save -> reload -> verify
        let elements = vec![
            crate::elements::Element::Heading { level: 1, text: "REPL Test".into() },
            crate::elements::Element::Paragraph { text: "First paragraph.".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        // "load" step
        let mut doc = PdfDocument::load_from_bytes(&pdf_bytes).unwrap();
        assert!(!doc.objects.is_empty(), "Should load document");

        // "text" step
        let text = doc.get_text().unwrap();
        assert!(text.contains("REPL Test"), "Text extraction should work");

        // "info" step
        assert_eq!(doc.version, "1.4", "Version should be 1.4");
        assert!(doc.catalog > 0, "Should have a catalog");

        // "sanitize" step
        doc.sanitize();

        // "attach" step
        doc.embed_file("note.txt", b"REPL session note").unwrap();

        // "save" step (serialize to bytes)
        let saved_bytes = doc.to_bytes();
        assert!(!saved_bytes.is_empty(), "Should serialize document");

        // "reload" step
        let reloaded = PdfDocument::load_from_bytes(&saved_bytes).unwrap();
        let reloaded_text = reloaded.get_text().unwrap();
        assert!(reloaded_text.contains("REPL Test"), "Text should survive round-trip");

        // "validate" step
        let validation = validate_pdf_bytes(&saved_bytes);
        assert!(validation.valid, "Round-tripped PDF should be valid: {:?}", validation.errors);
    }
}
