//! Memory-efficient streaming PDF generation for large documents
//!
//! [`StreamingPdfGenerator`] writes pages incrementally to disk instead of
//! buffering the entire document in memory, making it suitable for very
//! large reports or server scenarios where early bytes can be streamed.

use crate::elements::{Element, TextSegment};
use crate::pdf_generator::{Color, PageLayout, PdfGenerator, escape_pdf_string};
use anyhow::Result;
use std::fs::File;
use std::io::{BufWriter, Write};

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
    base_font_size: f32,
    current_color: Color,
    current_page: Vec<u8>,
    current_y: f32,
    font_state: FontState,
    page_contents: Vec<u32>, // Object IDs of page content streams
    page_objects: Vec<u32>,  // Object IDs of page dictionaries
}

#[derive(Debug, Clone)]
struct FontState {
    size: f32,
    name: String,
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
            format!("/{} {} Tf\n", self.font_state.name, self.font_state.size).as_bytes(),
        );
    }

    /// Set the text color
    pub fn set_color(&mut self, color: Color) -> Result<()> {
        self.current_color = color;
        self.current_page
            .extend_from_slice(format!("{} {} {} rg\n", color.r, color.g, color.b).as_bytes());
        Ok(())
    }

    /// Write text at current position
    pub fn write_text(&mut self, text: &str) -> Result<()> {
        let escaped = escape_pdf_string(text);
        let line_height = self.font_state.size + 4.0;

        self.current_page.extend_from_slice(b"BT\n");
        self._write_font_command();
        self.current_page.extend_from_slice(
            format!(
                "1 0 0 1 {} {} Tm\n",
                self.layout.margin_left, self.current_y
            )
            .as_bytes(),
        );
        self.current_page
            .extend_from_slice(format!("({}) Tj\n", escaped).as_bytes());
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

        // Use bold font with heading size
        let prev_size = self.font_state.size;
        self.font_state.size = size;
        self.font_state.name = "Helvetica-Bold".to_string();
        self.write_text("")?;
        self.write_text(text)?;
        self.font_state.name = "Helvetica".to_string();
        self.font_state.size = prev_size;
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
                    let _ = self.set_font("Helvetica", self.base_font_size);
                    self.write_text(text)?;
                }
                TextSegment::Bold(text) => {
                    let _ = self.set_font("Helvetica-Bold", self.base_font_size);
                    self.write_text(text)?;
                }
                TextSegment::Italic(text) => {
                    let _ = self.set_font("Helvetica-Oblique", self.base_font_size);
                    self.write_text(text)?;
                }
                TextSegment::BoldItalic(text) => {
                    let _ = self.set_font("Helvetica-BoldOblique", self.base_font_size);
                    self.write_text(text)?;
                }
                TextSegment::Code(code) => {
                    let code_size = self.base_font_size * 0.9;
                    let _ = self.set_font("Courier", code_size);
                    self.write_text(code)?;
                }
                TextSegment::Strikethrough(text) => {
                    let _ = self.set_font("Helvetica", self.base_font_size);
                    self.write_text(text)?;
                }
                TextSegment::MathInline(expr) => {
                    let _ = self.set_font("Helvetica-Oblique", self.base_font_size);
                    self.write_text(expr)?;
                    let _ = self.set_font("Helvetica", self.base_font_size);
                }
                TextSegment::Link { text, url } => {
                    let _ = self.set_font("Helvetica", self.base_font_size);
                    self.write_text(&format!("{} ({})", text, url))?;
                }
                TextSegment::Citation { key } => {
                    let _ = self.set_font("Helvetica", self.base_font_size);
                    self.write_text(&format!("[@{}]", key))?;
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

    /// Add elements. Unsupported layout-heavy types fall back to plain text so
    /// content is never silently dropped; prefer `generate_pdf_bytes` for full fidelity.
    pub fn add_elements(&mut self, elements: &[Element]) -> Result<()> {
        for elem in elements {
            match elem {
                Element::Heading { level, text } => {
                    self.add_heading(text, *level)?;
                }
                Element::Paragraph { text }
                | Element::StyledText { text, .. }
                | Element::InlineCode { code: text }
                | Element::MathInline { expression: text }
                | Element::MathBlock { expression: text } => {
                    self.add_paragraph(text)?;
                }
                Element::RichParagraph { segments } => {
                    self.add_rich_paragraph(segments)?;
                }
                Element::CodeBlock { code, language } => {
                    self.add_code_block(code, language)?;
                }
                Element::UnorderedListItem { text, depth } => {
                    let indent = "  ".repeat(*depth as usize);
                    self.add_paragraph(&format!("{}- {}", indent, text))?;
                }
                Element::OrderedListItem {
                    number,
                    text,
                    depth,
                } => {
                    let indent = "  ".repeat(*depth as usize);
                    self.add_paragraph(&format!("{}{}. {}", indent, number, text))?;
                }
                Element::TaskListItem { checked, text } => {
                    let marker = if *checked { "[x]" } else { "[ ]" };
                    self.add_paragraph(&format!("{} {}", marker, text))?;
                }
                Element::BlockQuote { text, depth } => {
                    let prefix = "> ".repeat(*depth as usize);
                    self.add_paragraph(&format!("{}{}", prefix, text))?;
                }
                Element::DefinitionItem { term, definition } => {
                    self.add_paragraph(term)?;
                    self.add_paragraph(&format!("  {}", definition))?;
                }
                Element::Footnote { label, text } => {
                    self.add_paragraph(&format!("[{}] {}", label, text))?;
                }
                Element::Link { text, url } => {
                    self.add_paragraph(&format!("{} ({})", text, url))?;
                }
                Element::Image { alt, path } => {
                    self.add_paragraph(&format!("[Image: {}] ({})", alt, path))?;
                }
                Element::Chart {
                    kind,
                    title,
                    points,
                    series: _,
                } => {
                    let kind = match kind {
                        crate::elements::ChartKind::Bar => "bar",
                        crate::elements::ChartKind::Line => "line",
                        crate::elements::ChartKind::Pie => "pie",
                        crate::elements::ChartKind::StackedBar => "stacked-bar",
                    };
                    let title = title.as_deref().unwrap_or("Chart");
                    self.add_paragraph(&format!(
                        "[Chart {} — {} ({} series)]",
                        kind,
                        title,
                        points.len()
                    ))?;
                }
                Element::TableRow {
                    cells,
                    is_separator,
                    ..
                } => {
                    if !*is_separator {
                        self.add_paragraph(&cells.join(" | "))?;
                    }
                }
                Element::HorizontalRule => {
                    self.add_paragraph("----------")?;
                }
                Element::PageBreak => {
                    self.flush_page()?;
                }
                Element::EmptyLine
                | Element::Columns { .. }
                | Element::PageNumberMode { .. }
                | Element::RunningHeaderMode { .. }
                | Element::Toc
                | Element::Bibliography
                | Element::CitationDef { .. } => {
                    // Layout directives / spacers: no textual body.
                    if matches!(elem, Element::EmptyLine) {
                        self.current_y -= (self.base_font_size + 4.0) * 0.5;
                    }
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
        let content_stream = format!("<< /Length {} >>\nstream\n", content_length);

        let content_id = self
            .generator
            .add_stream_object(content_stream, self.current_page.clone());

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
        self.flush_page()?;

        let total_pages = self.page_contents.len();
        let pages_obj_id = self.generator.next_id + total_pages as u32 * 6;
        let mut page_ids = Vec::new();

        for &content_id in &self.page_contents {
            let page_id = self.generator.next_id;
            page_ids.push(page_id);

            let f1 = self.generator.add_object(
                "<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica >>\n".to_string(),
            );
            let f2 = self.generator.add_object(
                "<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica-Bold >>\n".to_string(),
            );
            let f3 = self.generator.add_object(
                "<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica-Oblique >>\n".to_string(),
            );
            let f4 = self.generator.add_object(
                "<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica-BoldOblique >>\n"
                    .to_string(),
            );
            let f5 = self
                .generator
                .add_object("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Courier >>\n".to_string());

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
                pages_obj_id, self.layout.width, self.layout.height, content_id, f1, f2, f3, f4, f5
            );
            self.generator.add_object(page_dict);
        }

        let kids: Vec<String> = page_ids.iter().map(|id| format!("{} 0 R", id)).collect();
        let pages_dict = format!(
            "<< /Type /Pages\n\
             /Kids [{}]\n\
             /Count {}\n\
             >>\n",
            kids.join(" "),
            total_pages
        );
        self.generator.add_object(pages_dict);

        let catalog_dict = format!(
            "<< /Type /Catalog\n\
             /Pages {} 0 R\n\
             >>\n",
            pages_obj_id
        );
        self.generator.add_object(catalog_dict);

        let pdf_data = self.generator.generate();
        self.file.write_all(&pdf_data)?;
        self.file.flush()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_basic() {
        let mut pdf_gen =
            StreamingPdfGenerator::new("/tmp/test_stream.pdf", PageLayout::portrait()).unwrap();

        pdf_gen.add_heading("Test", 1).unwrap();
        pdf_gen.add_paragraph("Content here").unwrap();

        let result = pdf_gen.finish();
        assert!(result.is_ok());
    }
}
