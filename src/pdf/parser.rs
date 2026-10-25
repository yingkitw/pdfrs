//! PDF parsing: object streams, xref streams, dictionaries, and lazy
//! document loading that indexes streams without full parsing.

use super::decode::find_subsequence;
use super::objects::{PdfDocument, PdfObject, PdfValue};
use super::{re_obj, re_root, re_tj, re_tj_array, re_tj_hex, re_tj_hex_str, re_tj_str};
use crate::error::{PdfError, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

pub(super) fn find_stream_ranges(buffer: &[u8]) -> Vec<(usize, usize)> {
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

/// Check if bytes form a valid zlib header (CMF=0x78, FLG satisfies checksum)
pub(super) fn decompress_stream(data: &[u8]) -> Vec<u8> {
    crate::search::decompress_stream(data)
}

// --- Object parsing ---

pub(super) fn parse_objects(content: &str, doc: &mut PdfDocument) -> Result<()> {
    let obj_re = re_obj();
    let mut lines = content.lines();

    while let Some(line) = lines.next() {
        let line = line.trim();

        if let Some(caps) = obj_re.captures(line) {
            // Only match if the line is exactly "N G obj" (possibly with trailing whitespace)
            let full_match = caps.get(0).unwrap().as_str();
            if (line == full_match || line.starts_with(full_match))
                && let (Ok(obj_num), Ok(_gen_num)) =
                    (caps[1].parse::<u32>(), caps[2].parse::<u32>())
            {
                let mut obj_content = String::new();

                for inner in lines.by_ref() {
                    if inner.trim().starts_with("endobj") {
                        break;
                    }
                    obj_content.push_str(inner);
                    obj_content.push('\n');
                }

                let obj = parse_object_content(&obj_content)?;
                doc.objects.insert(obj_num, obj);
            }
        }
    }

    Ok(())
}

pub(super) fn parse_object_content(content: &str) -> Result<PdfObject> {
    let content = content.trim();

    // Check for stream objects: dictionary followed by stream data
    if let (Some(stream_pos), Some(endstream_pos)) =
        (content.find("\nstream\n"), content.find("\nendstream"))
    {
        let dict_part = content[..stream_pos].trim();
        let data_start = stream_pos + "\nstream\n".len();
        let data = content.as_bytes()[data_start..endstream_pos].to_vec();

        let dict = parse_dict_entries(dict_part);

        Ok(PdfObject::Stream {
            dictionary: dict,
            data,
        })
    } else if content.contains("stream") && content.contains("endstream") {
        let stream_idx = content
            .find("stream")
            .ok_or_else(|| PdfError::Parse("stream keyword not found".into()))?;
        let endstream_idx = content
            .find("endstream")
            .ok_or_else(|| PdfError::Parse("endstream keyword not found".into()))?;
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
        Ok(PdfObject::String(content[1..content.len() - 1].to_string()))
    } else {
        Ok(PdfObject::String(content.to_string()))
    }
}

/// Parse dictionary entries from << ... >> content
pub(super) fn parse_dict_entries(raw: &str) -> HashMap<String, PdfValue> {
    let mut dict = HashMap::new();
    let inner = raw.trim().trim_start_matches("<<").trim_end_matches(">>");
    let tokens: Vec<&str> = inner.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].starts_with('/') {
            let key = tokens[i][1..].to_string();
            i += 1;
            if i < tokens.len() {
                let val = tokens[i].to_string();
                dict.insert(key, PdfValue::Object(PdfObject::String(val)));
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
pub(super) fn read_xref_field(data: &[u8], offset: usize, width: usize) -> u64 {
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
            && header.starts_with("%PDF-")
        {
            version = header[5..].to_string();
        }

        let catalog = {
            let root_re = re_root();
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
        let obj_re = re_obj();
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
        let tj_re = re_tj();
        let tj_hex_re = re_tj_hex();
        let tj_array_re = re_tj_array();
        let tj_str_re = re_tj_str();
        let tj_hex_str_re = re_tj_hex_str();

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
                            && let Ok(s) = String::from_utf8(bytes)
                        {
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
                                    && let Ok(s) = String::from_utf8(bytes)
                                {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dict_entries() {
        let raw = "<< /Type /Page /Length 42 >>";
        let dict = parse_dict_entries(raw);
        assert!(dict.contains_key("Type"));
        assert!(dict.contains_key("Length"));
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
}
