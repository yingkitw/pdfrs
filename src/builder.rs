use crate::elements::Element;
use crate::pdf_generator::{PageLayout, create_pdf_from_elements_with_layout};
use anyhow::Result;

/// Fluent builder for creating PDFs with a clean, ergonomic API
///
/// # Example
/// ```rust,no_run
/// use pdf_rs::builder::PdfBuilder;
/// use pdf_rs::pdf_generator::PageLayout;
///
/// let pdf = PdfBuilder::new()
///     .with_layout(PageLayout::landscape())
///     .with_margins(72.0)
///     .add_heading("My Document", 1)
///     .add_paragraph("This is the content.")
///     .add_code_block("fn main() {}", "rust")
///     .build("output.pdf");
/// ```
pub struct PdfBuilder {
    elements: Vec<Element>,
    layout: PageLayout,
    font: String,
    font_size: f32,
}

impl PdfBuilder {
    /// Create a new PDF builder with default settings
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            layout: PageLayout::portrait(),
            font: "Helvetica".to_string(),
            font_size: 12.0,
        }
    }

    /// Set the page layout (orientation and margins)
    pub fn with_layout(mut self, layout: PageLayout) -> Self {
        self.layout = layout;
        self
    }

    /// Set uniform margins for all sides
    pub fn with_margins(mut self, margin: f32) -> Self {
        self.layout = PageLayout {
            margin_left: margin,
            margin_right: margin,
            margin_top: margin,
            margin_bottom: margin,
            ..self.layout
        };
        self
    }

    /// Set individual margins
    pub fn with_custom_margins(mut self, left: f32, right: f32, top: f32, bottom: f32) -> Self {
        self.layout = PageLayout {
            margin_left: left,
            margin_right: right,
            margin_top: top,
            margin_bottom: bottom,
            ..self.layout
        };
        self
    }

    /// Set the base font family
    pub fn with_font(mut self, font: &str) -> Self {
        self.font = font.to_string();
        self
    }

    /// Set the base font size
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Add a heading element
    pub fn add_heading(mut self, text: &str, level: u8) -> Self {
        self.elements.push(Element::Heading {
            text: text.to_string(),
            level,
        });
        self
    }

    /// Add a paragraph element
    pub fn add_paragraph(mut self, text: &str) -> Self {
        self.elements.push(Element::Paragraph {
            text: text.to_string(),
        });
        self
    }

    /// Add a code block with syntax highlighting
    pub fn add_code_block(mut self, code: &str, language: &str) -> Self {
        self.elements.push(Element::CodeBlock {
            code: code.to_string(),
            language: language.to_string(),
        });
        self
    }

    /// Add an unordered list item
    pub fn add_list_item(mut self, text: &str, depth: u8) -> Self {
        self.elements.push(Element::UnorderedListItem {
            text: text.to_string(),
            depth,
        });
        self
    }

    /// Add an ordered list item
    pub fn add_ordered_item(mut self, number: usize, text: &str, depth: u8) -> Self {
        self.elements.push(Element::OrderedListItem {
            number: number as u32,
            text: text.to_string(),
            depth,
        });
        self
    }

    /// Add a task list item (checkbox)
    pub fn add_task_item(mut self, text: &str, checked: bool) -> Self {
        self.elements.push(Element::TaskListItem {
            text: text.to_string(),
            checked,
        });
        self
    }

    /// Add a table row (helper for building tables)
    pub fn add_table_row(mut self, cells: &[&str]) -> Self {
        self.elements.push(Element::TableRow {
            cells: cells.iter().map(|s| s.to_string()).collect(),
            is_separator: false,
            alignments: vec![],
        });
        self
    }

    /// Add a table separator (header row delimiter)
    pub fn add_table_separator(mut self, cells: &[&str]) -> Self {
        self.elements.push(Element::TableRow {
            cells: cells.iter().map(|s| s.to_string()).collect(),
            is_separator: true,
            alignments: vec![],
        });
        self
    }

    /// Add a horizontal rule
    pub fn add_horizontal_rule(mut self) -> Self {
        self.elements.push(Element::HorizontalRule);
        self
    }

    /// Add a page break
    pub fn add_page_break(mut self) -> Self {
        self.elements.push(Element::PageBreak);
        self
    }

    /// Add an empty line
    pub fn add_spacing(mut self) -> Self {
        self.elements.push(Element::EmptyLine);
        self
    }

    /// Add a block quote
    pub fn add_blockquote(mut self, text: &str, depth: u8) -> Self {
        self.elements.push(Element::BlockQuote {
            text: text.to_string(),
            depth,
        });
        self
    }

    /// Add a link
    pub fn add_link(mut self, text: &str, url: &str) -> Self {
        self.elements.push(Element::Link {
            text: text.to_string(),
            url: url.to_string(),
        });
        self
    }

    /// Add an image reference
    pub fn add_image(mut self, alt: &str, path: &str) -> Self {
        self.elements.push(Element::Image {
            alt: alt.to_string(),
            path: path.to_string(),
        });
        self
    }

    /// Add a definition (term and definition)
    pub fn add_definition(mut self, term: &str, definition: &str) -> Self {
        self.elements.push(Element::DefinitionItem {
            term: term.to_string(),
            definition: definition.to_string(),
        });
        self
    }

    /// Add a footnote
    pub fn add_footnote(mut self, label: &str, text: &str) -> Self {
        self.elements.push(Element::Footnote {
            label: label.to_string(),
            text: text.to_string(),
        });
        self
    }

    /// Add styled text (bold/italic)
    pub fn add_styled_text(mut self, text: &str, bold: bool, italic: bool) -> Self {
        self.elements.push(Element::StyledText {
            text: text.to_string(),
            bold,
            italic,
        });
        self
    }

    /// Add inline code
    pub fn add_inline_code(mut self, code: &str) -> Self {
        self.elements.push(Element::InlineCode {
            code: code.to_string(),
        });
        self
    }

    /// Add a math block
    pub fn add_math_block(mut self, expression: &str) -> Self {
        self.elements.push(Element::MathBlock {
            expression: expression.to_string(),
        });
        self
    }

    /// Add inline math
    pub fn add_inline_math(mut self, expression: &str) -> Self {
        self.elements.push(Element::MathInline {
            expression: expression.to_string(),
        });
        self
    }

    /// Add multiple elements from an iterator
    pub fn add_elements(mut self, elements: impl IntoIterator<Item = Element>) -> Self {
        for elem in elements {
            self.elements.push(elem);
        }
        self
    }

    /// Add a raw element (for advanced usage)
    pub fn add_element(mut self, element: Element) -> Self {
        self.elements.push(element);
        self
    }

    /// Build the PDF and write to a file
    pub fn build(self, filename: &str) -> Result<()> {
        create_pdf_from_elements_with_layout(
            filename,
            &self.elements,
            &self.font,
            self.font_size,
            self.layout,
        )
    }

    /// Build the PDF and return the bytes (no filesystem access)
    pub fn build_bytes(self) -> Result<Vec<u8>> {
        crate::pdf_generator::generate_pdf_bytes(
            &self.elements,
            &self.font,
            self.font_size,
            self.layout,
        )
    }

    /// Get the current element count
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// Clear all elements
    pub fn clear(mut self) -> Self {
        self.elements.clear();
        self
    }
}

impl Default for PdfBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_basic() {
        let pdf = PdfBuilder::new()
            .add_heading("Test Document", 1)
            .add_paragraph("This is a test.")
            .add_code_block("let x = 42;", "rust")
            .build_bytes();

        assert!(pdf.is_ok());
        let pdf_bytes = pdf.unwrap();
        assert!(pdf_bytes.len() > 100);
        assert!(pdf_bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_builder_with_layout() {
        let pdf = PdfBuilder::new()
            .with_layout(PageLayout::landscape())
            .with_margins(50.0)
            .add_heading("Landscape", 1)
            .build_bytes();

        assert!(pdf.is_ok());
    }

    #[test]
    fn test_builder_count() {
        let builder = PdfBuilder::new()
            .add_heading("Title", 1)
            .add_paragraph("Content")
            .add_spacing();

        assert_eq!(builder.element_count(), 3);
    }

    #[test]
    fn test_builder_clear() {
        let builder = PdfBuilder::new()
            .add_heading("Title", 1)
            .add_paragraph("Content")
            .clear();

        assert_eq!(builder.element_count(), 0);
    }

    #[test]
    fn test_builder_table() {
        let pdf = PdfBuilder::new()
            .add_table_row(&["Name", "Age", "City"])
            .add_table_separator(&["----", "----", "----"])
            .add_table_row(&["Alice", "30", "NYC"])
            .add_table_row(&["Bob", "25", "LA"])
            .build_bytes();

        assert!(pdf.is_ok());
    }
}
