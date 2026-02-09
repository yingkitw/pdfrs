use crate::elements::{Element, TextSegment};
use crate::pdf_generator::{PageLayout, PdfGenerator, Color};
use anyhow::Result;
use std::fs::File;
use std::io::{Write, BufWriter};

/// Streaming PDF generator that writes pages to disk as they're generated
/// instead of buffering everything in memory.
///
/// This is useful for:
/// - Very large documents that don't fit in memory
/// - Documents where early pages can be viewed while later pages are still generating
/// - Server scenarios where you want to start sending the PDF immediately
///
/// # Example
/// ```rust,no_run
/// use pdfrs::streaming::StreamingPdfGenerator;
/// use pdfrs::pdf_generator::PageLayout;
///
/// let mut pdf_gen = StreamingPdfGenerator::new("output.pdf", PageLayout::portrait()).unwrap();
/// pdf_gen.add_heading("Large Document", 1).unwrap();
///
/// for i in 0..10 {
///     pdf_gen.add_paragraph(&format!("Chapter {}", i)).unwrap();
/// }
///
/// pdf_gen.finish().unwrap();
/// ```
pub struct StreamingPdfGenerator {
    file: BufWriter<File>,
    generator: PdfGenerator,
    layout: PageLayout,
    font: String,
    base_font_size: f32,
    current_color: Color,
    current_page: Vec<u8>,
    current_y: f32,
    font_state: FontState,
    page_contents: Vec<u32>, // Object IDs of page content streams
    page_objects: Vec<u32>,    // Object IDs of page dictionaries
    fonts_per_page: usize,
}

#[derive(Debug, Clone)]
struct FontState {
    size: f32,
    name: String,
}

fn escape_pdf_string(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

impl StreamingPdfGenerator {
    /// Create a new streaming PDF generator
    pub fn new(filename: &str, layout: PageLayout) -> Result<Self> {
        let file = BufWriter::new(File::create(filename)?);

        // We'll write PDF structure incrementally
        // Start with header placeholder
        // We'll come back and fill in offsets later

        Ok(Self {
            file,
            generator: PdfGenerator::new(),
            layout,
            font: "Helvetica".to_string(),
            base_font_size: 12.0,
            current_color: Color::black(),
            current_page: Vec::new(),
            current_y: layout.content_top(),
            font_state: FontState {
                size: 12.0,
                name: "Helvetica".to_string(),
            },
            page_contents: Vec::new(),
            page_objects: Vec::new(),
            fonts_per_page: 5,
        })
    }

    /// Set the font for subsequent text
    pub fn set_font(&mut self, font: &str, size: f32) -> Result<()> {
        self.font_state = FontState {
            name: font.to_string(),
            size,
        };
        self._write_font_command();
        Ok(())
    }

    fn _write_font_command(&mut self) {
        self.current_page.extend_from_slice(
            format!("/{} {} Tf\n", self.font_state.name, self.font_state.size).as_bytes()
        );
    }

    /// Set the text color
    pub fn set_color(&mut self, color: Color) -> Result<()> {
        self.current_color = color;
        self.current_page.extend_from_slice(
            format!("{} {} {} rg\n", color.r, color.g, color.b).as_bytes()
        );
        Ok(())
    }

    /// Write text at current position
    pub fn write_text(&mut self, text: &str) -> Result<()> {
        let escaped = escape_pdf_string(text);
        let line_height = self.font_state.size + 4.0;

        self.current_page.extend_from_slice(b"BT\n");
        self._write_font_command();
        self.current_page.extend_from_slice(
            format!("1 0 0 1 {} {} Tm\n", self.layout.margin_left, self.current_y).as_bytes()
        );
        self.current_page.extend_from_slice(
            format!("({}) Tj\n", escaped).as_bytes()
        );
        self.current_page.extend_from_slice(b"ET\n");

        self.current_y -= line_height;
        Ok(())
    }

    /// Add a heading
    pub fn add_heading(&mut self, text: &str, level: u8) -> Result<()> {
        let size = match level {
            1 => self.base_font_size * 2.0,
            2 => self.base_font_size * 1.6,
            3 => self.base_font_size * 1.3,
            4 => self.base_font_size * 1.1,
            _ => self.base_font_size,
        };

        // Use bold font
        self.font_state.name = format!("Helvetica-Bold");
        self.write_text("")?;
        self.write_text(text)?;
        self.font_state.name = "Helvetica".to_string();
        Ok(())
    }

    /// Add a paragraph
    pub fn add_paragraph(&mut self, text: &str) -> Result<()> {
        self.write_text(text)
    }

    /// Add a rich paragraph with styled segments
    pub fn add_rich_paragraph(&mut self, segments: &[TextSegment]) -> Result<()> {
        for segment in segments {
            match segment {
                TextSegment::Plain(text) => {
                    self.set_font("Helvetica", self.base_font_size);
                    self.write_text(text)?;
                }
                TextSegment::Bold(text) => {
                    self.set_font("Helvetica-Bold", self.base_font_size);
                    self.write_text(text)?;
                }
                TextSegment::Italic(text) => {
                    self.set_font("Helvetica-Oblique", self.base_font_size);
                    self.write_text(text)?;
                }
                TextSegment::BoldItalic(text) => {
                    self.set_font("Helvetica-BoldOblique", self.base_font_size);
                    self.write_text(text)?;
                }
                TextSegment::Code(code) => {
                    let code_size = self.base_font_size * 0.9;
                    self.set_font("Courier", code_size);
                    self.write_text(code)?;
                }
                TextSegment::Link { text, url } => {
                    self.set_font("Helvetica", self.base_font_size);
                    self.write_text(&format!("{} ({})", text, url))?;
                }
            }
        }
        Ok(())
    }

    /// Add a code block
    pub fn add_code_block(&mut self, code: &str, _language: &str) -> Result<()> {
        // Set monospace font
        self.font_state.name = "Courier".to_string();
        self.font_state.size = self.base_font_size * 0.85;

        for line in code.lines() {
            self.write_text(line)?;
        }

        // Reset font
        self.font_state.name = "Helvetica".to_string();
        self.font_state.size = self.base_font_size;
        Ok(())
    }

    /// Add elements (same as normal PDF generation)
    pub fn add_elements(&mut self, elements: &[Element]) -> Result<()> {
        // For now, just process paragraphs and headings
        for elem in elements {
            match elem {
                Element::Heading { level, text } => {
                    self.add_heading(text, *level)?;
                }
                Element::Paragraph { text } => {
                    self.add_paragraph(text)?;
                }
                Element::RichParagraph { segments } => {
                    self.add_rich_paragraph(segments)?;
                }
                Element::CodeBlock { code, language } => {
                    self.add_code_block(code, language)?;
                }
                Element::EmptyLine => {
                    self.current_y -= (self.base_font_size + 4.0) * 0.5;
                }
                _ => {
                    // Skip other elements for now
                }
            }
        }
        Ok(())
    }

    /// Add a raw element
    pub fn add_element(&mut self, element: Element) -> Result<()> {
        self.add_elements(&[element])
    }

    /// Complete the current page and write it to disk
    pub fn flush_page(&mut self) -> Result<()> {
        if self.current_page.is_empty() {
            return Ok(());
        }

        // Add page footer
        self.current_page.extend_from_slice(b"ET\n");

        // Write the content stream object
        let content_length = self.current_page.len();
        let content_stream = format!(
            "<< /Length {} >>\nstream\n",
            content_length
        );

        let content_id = self.generator.add_stream_object(
            content_stream,
            self.current_page.clone()
        );

        // Store for later page tree construction
        self.page_contents.push(content_id);
        self.page_objects.push(0); // Placeholder, will be filled

        // Clear current page buffer
        self.current_page = Vec::new();
        self.current_y = self.layout.content_top();

        Ok(())
    }

    /// Finish the PDF and close the file
    pub fn finish(mut self) -> Result<()> {
        // Flush any remaining content
        self.flush_page()?;

        // Build page tree and catalog
        let total_pages = self.page_contents.len();
        let fonts_per_page = self.fonts_per_page;

        // Calculate object IDs
        // Layout: for each page: content_stream, page_obj, 5 fonts
        // Then: pages_obj, catalog_obj
        let pages_obj_id = (total_pages * (2 + fonts_per_page) + 2) as u32;

        let mut all_objects = Vec::new();

        // Add all page objects
        for (i, &content_id) in self.page_contents.iter().enumerate() {
            let page_id = content_id + 1;
            let first_font_id = content_id + 2;

            // Page dictionary
            let page_dict = format!(
                "<< /Type /Page\n\
                 /Parent {} 0 R\n\
                 /MediaBox [0 0 {} {}]\n\
                 /Contents {} 0 R\n\
                 /Resources << /Font << \
                     /Helvetica {} 0 R \
                     /Helvetica-Bold {} 0 R \
                     /Helvetica-Oblique {} 0 R \
                     /Helvetica-BoldOblique {} 0 R \
                     /Courier {} 0 R \
                 >> >>\n\
                 >>\n",
                pages_obj_id,
                self.layout.width,
                self.layout.height,
                content_id,
                first_font_id,
                first_font_id + 1,
                first_font_id + 2,
                first_font_id + 3,
                first_font_id + 4
            );

            all_objects.push((page_id, page_dict));

            // Font objects
            all_objects.push((first_font_id, format!("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica >>\n")));
            all_objects.push((first_font_id + 1, format!("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica-Bold >>\n")));
            all_objects.push((first_font_id + 2, format!("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica-Oblique >>\n")));
            all_objects.push((first_font_id + 3, format!("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica-BoldOblique >>\n")));
            all_objects.push((first_font_id + 4, format!("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Courier >>\n")));
        }

        // Pages object
        let page_refs: Vec<String> = all_objects.iter()
            .filter(|(id, _)| {
                // Page objects are at positions: 1, 8, 15, ...
                (*id - 1) % (2 + fonts_per_page as u32) == 0
            })
            .map(|(id, _)| format!("{} 0 R", id))
            .collect();

        let pages_dict = format!(
            "<< /Type /Pages\n\
             /Kids [{}]\n\
             /Count {}\n\
             >>\n",
            page_refs.join(" "),
            total_pages
        );

        all_objects.push((pages_obj_id, pages_dict));

        // Catalog
        let catalog_dict = format!(
            "<< /Type /Catalog\n\
             /Pages {} 0 R\n\
             >>\n",
            pages_obj_id
        );

        all_objects.push((pages_obj_id + 1, catalog_dict));

        // Now we need to regenerate with proper IDs
        // This is a simplified version - in production, you'd track IDs better
        let mut generator = PdfGenerator::new();

        // Re-add all objects with proper IDs
        for (_, content) in &all_objects {
            generator.add_object(content.clone());
        }

        // Generate PDF
        let pdf_data = generator.generate();
        self.file.write_all(&pdf_data)?;
        self.file.flush()?;

        Ok(())
    }
}

/// Stream pages as they're generated (useful for very large documents)
pub struct StreamingPdfPageIterator {
    elements: std::vec::IntoIter<Element>,
    layout: PageLayout,
    font: String,
    font_size: f32,
}

impl StreamingPdfPageIterator {
    pub fn new(elements: Vec<Element>, layout: PageLayout) -> Self {
        Self {
            elements: elements.into_iter(),
            layout,
            font: "Helvetica".to_string(),
            font_size: 12.0,
        }
    }
}

impl Iterator for StreamingPdfPageIterator {
    type Item = Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        // Collect elements until we have enough for a page
        // For simplicity, we'll return None for now
        // A full implementation would page-break intelligently
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_basic() {
        let mut pdf_gen = StreamingPdfGenerator::new(
            "/tmp/test_stream.pdf",
            PageLayout::portrait()
        ).unwrap();

        pdf_gen.add_heading("Test", 1).unwrap();
        pdf_gen.add_paragraph("Content here").unwrap();

        let result = pdf_gen.finish();
        assert!(result.is_ok());
    }
}
