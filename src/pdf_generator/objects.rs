//! The low-level PDF object model: [`PdfGenerator`] and [`PdfObj`].
//!
//! [`PdfGenerator`] accumulates PDF objects (dictionaries and streams) and
//! serialises them into a complete PDF file with a cross-reference table.
//! It is the building block used by every higher-level generation path.

use super::layout::PdfVersion;

pub struct PdfGenerator {
    pub objects: Vec<PdfObj>,
    pub next_id: u32,
    pub info_id: Option<u32>,
    pub version: PdfVersion,
}

#[derive(Debug)]
pub struct PdfObj {
    pub id: u32,
    pub generation: u32,
    pub content: String,
    pub is_stream: bool,
    pub stream_data: Option<Vec<u8>>,
}

impl Default for PdfGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfGenerator {
    pub fn new() -> Self {
        PdfGenerator {
            objects: Vec::new(),
            next_id: 1,
            info_id: None,
            version: PdfVersion::default(),
        }
    }

    pub fn with_version(mut self, version: PdfVersion) -> Self {
        self.version = version;
        self
    }

    pub fn add_object(&mut self, content: String) -> u32 {
        let id = self.next_id;
        self.objects.push(PdfObj {
            id,
            generation: 0,
            content,
            is_stream: false,
            stream_data: None,
        });
        self.next_id += 1;
        id
    }

    pub fn add_stream_object(&mut self, dictionary: String, data: Vec<u8>) -> u32 {
        let id = self.next_id;
        self.objects.push(PdfObj {
            id,
            generation: 0,
            content: dictionary,
            is_stream: true,
            stream_data: Some(data),
        });
        self.next_id += 1;
        id
    }

    pub fn generate(&self) -> Vec<u8> {
        let mut pdf = Vec::new();

        // PDF header
        pdf.extend_from_slice(self.version.header());

        // Calculate offsets for xref table
        let mut offsets = Vec::new();
        let mut current_offset = pdf.len() as u32;

        // Write objects and collect offsets
        for obj in &self.objects {
            offsets.push(current_offset);
            let obj_header = format!("{} {} obj\n", obj.id, obj.generation);
            pdf.extend_from_slice(obj_header.as_bytes());
            pdf.extend_from_slice(obj.content.as_bytes());

            if obj.is_stream
                && let Some(data) = &obj.stream_data
            {
                pdf.extend_from_slice(b"stream\n");
                pdf.extend_from_slice(data);
                pdf.extend_from_slice(b"\nendstream\n");
            }

            pdf.extend_from_slice(b"endobj\n");
            current_offset = pdf.len() as u32;
        }

        // xref table
        let xref_offset = pdf.len() as u32;
        pdf.extend_from_slice(format!("xref\n0 {}\n", self.objects.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");

        for offset in offsets {
            pdf.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
        }

        // trailer
        pdf.extend_from_slice(b"trailer\n");
        pdf.extend_from_slice(b"<<\n");
        pdf.extend_from_slice(format!("/Size {}\n", self.objects.len() + 1).as_bytes());
        if !self.objects.is_empty() {
            pdf.extend_from_slice(format!("/Root {} 0 R\n", self.objects.len()).as_bytes());
        }
        if let Some(info_id) = self.info_id {
            pdf.extend_from_slice(format!("/Info {} 0 R\n", info_id).as_bytes());
        }
        pdf.extend_from_slice(b">>\n");
        pdf.extend_from_slice(b"startxref\n");
        pdf.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        pdf.extend_from_slice(b"%%EOF\n");

        pdf
    }
}
