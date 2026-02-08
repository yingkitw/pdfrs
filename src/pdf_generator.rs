use crate::elements::Element;
use anyhow::Result;
use std::fs::File;
use std::io::Write;

// --- Page orientation and layout ---

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageOrientation {
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy)]
pub struct PageLayout {
    pub width: f32,
    pub height: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
}

impl PageLayout {
    pub fn portrait() -> Self {
        PageLayout {
            width: 612.0,
            height: 792.0,
            margin_left: 72.0,
            margin_right: 72.0,
            margin_top: 72.0,
            margin_bottom: 72.0,
        }
    }

    pub fn landscape() -> Self {
        PageLayout {
            width: 792.0,
            height: 612.0,
            margin_left: 72.0,
            margin_right: 72.0,
            margin_top: 72.0,
            margin_bottom: 72.0,
        }
    }

    pub fn from_orientation(orientation: PageOrientation) -> Self {
        match orientation {
            PageOrientation::Portrait => Self::portrait(),
            PageOrientation::Landscape => Self::landscape(),
        }
    }

    pub fn content_top(&self) -> f32 {
        self.height - self.margin_top
    }

    pub fn content_width(&self) -> f32 {
        self.width - self.margin_left - self.margin_right
    }
}

// --- Font size helpers ---
fn heading_font_size(level: u8, base: f32) -> f32 {
    match level {
        1 => base * 2.0,
        2 => base * 1.6,
        3 => base * 1.3,
        4 => base * 1.1,
        5 => base * 1.0,
        _ => base * 0.9,
    }
}

fn line_height(font_size: f32) -> f32 {
    font_size + 4.0
}

// --- Low-level PDF object model ---

pub struct PdfGenerator {
    pub objects: Vec<PdfObj>,
    pub next_id: u32,
}

#[derive(Debug)]
pub struct PdfObj {
    pub id: u32,
    pub generation: u32,
    pub content: String,
    pub is_stream: bool,
    pub stream_data: Option<Vec<u8>>,
}

impl PdfGenerator {
    pub fn new() -> Self {
        PdfGenerator {
            objects: Vec::new(),
            next_id: 1,
        }
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
        pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

        // Calculate offsets for xref table
        let mut offsets = Vec::new();
        let mut current_offset = pdf.len() as u32;

        // Write objects and collect offsets
        for obj in &self.objects {
            offsets.push(current_offset);
            let obj_header = format!("{} {} obj\n", obj.id, obj.generation);
            pdf.extend_from_slice(obj_header.as_bytes());
            pdf.extend_from_slice(obj.content.as_bytes());

            if obj.is_stream {
                if let Some(data) = &obj.stream_data {
                    pdf.extend_from_slice(b"stream\n");
                    pdf.extend_from_slice(data);
                    pdf.extend_from_slice(b"\nendstream\n");
                }
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
        pdf.extend_from_slice(b">>\n");
        pdf.extend_from_slice(b"startxref\n");
        pdf.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        pdf.extend_from_slice(b"%%EOF\n");

        pdf
    }
}

// --- Content stream builder (handles cursor, page breaks, font switches) ---

struct ContentStreamBuilder {
    pages: Vec<Vec<u8>>,
    current: Vec<u8>,
    y: f32,
    base_font_size: f32,
    current_font_size: f32,
    page_number: u32,
    total_pages: u32,
    show_page_numbers: bool,
    layout: PageLayout,
}

impl ContentStreamBuilder {
    fn new(base_font_size: f32, show_page_numbers: bool, layout: PageLayout) -> Self {
        let mut b = ContentStreamBuilder {
            pages: Vec::new(),
            current: Vec::new(),
            y: layout.content_top(),
            base_font_size,
            current_font_size: base_font_size,
            page_number: 1,
            total_pages: 0,
            show_page_numbers,
            layout,
        };
        b.begin_page();
        b
    }

    fn begin_page(&mut self) {
        self.current.clear();
        self.y = self.layout.content_top();
        self.current.extend_from_slice(b"BT\n");
        self.set_font(self.base_font_size);
        self.current
            .extend_from_slice(format!("{} {} Td\n", self.layout.margin_left, self.layout.content_top()).as_bytes());
    }

    fn set_font(&mut self, size: f32) {
        if (self.current_font_size - size).abs() > 0.01 {
            self.current_font_size = size;
            self.current
                .extend_from_slice(format!("/F1 {} Tf\n", size).as_bytes());
        }
    }

    fn needs_page_break(&self, extra: f32) -> bool {
        self.y - extra < self.layout.margin_bottom
    }

    fn new_page(&mut self) {
        self.end_text_block();
        self.pages.push(self.current.clone());
        self.page_number += 1;
        self.begin_page();
    }

    fn end_text_block(&mut self) {
        self.current.extend_from_slice(b"ET\n");
        if self.show_page_numbers {
            self.write_page_number();
        }
    }

    fn write_page_number(&mut self) {
        let label = format!("Page {}", self.page_number);
        let x = self.layout.width / 2.0 - 20.0;
        let y = self.layout.margin_bottom / 2.0;
        self.current.extend_from_slice(b"BT\n");
        self.current
            .extend_from_slice(format!("/F1 9 Tf\n").as_bytes());
        self.current
            .extend_from_slice(format!("{} {} Td\n", x, y).as_bytes());
        self.current
            .extend_from_slice(format!("({}) Tj\n", escape_pdf_string(&label)).as_bytes());
        self.current.extend_from_slice(b"ET\n");
    }

    fn emit_line(&mut self, text: &str, font_size: f32) {
        let lh = line_height(font_size);
        if self.needs_page_break(lh) {
            self.new_page();
        }
        self.set_font(font_size);
        let escaped = escape_pdf_string(text);
        self.current
            .extend_from_slice(format!("({}) Tj\n", escaped).as_bytes());
        self.y -= lh;
        self.current
            .extend_from_slice(format!("{} {} Td\n", self.layout.margin_left, self.y).as_bytes());
    }

    fn emit_empty_line(&mut self) {
        let lh = line_height(self.base_font_size) * 0.5;
        if self.needs_page_break(lh) {
            self.new_page();
        }
        self.y -= lh;
        self.current
            .extend_from_slice(format!("{} {} Td\n", self.layout.margin_left, self.y).as_bytes());
    }

    fn emit_horizontal_rule(&mut self) {
        self.emit_empty_line();
        self.emit_line("---", self.base_font_size);
        self.emit_empty_line();
    }

    fn finish(mut self) -> Vec<Vec<u8>> {
        self.end_text_block();
        self.pages.push(self.current);
        self.pages
    }
}

// --- Public API ---

pub fn create_pdf(filename: &str, text: &str) -> Result<()> {
    create_pdf_with_options(filename, text, "Helvetica", 12.0)
}

/// Legacy plain-text pipeline (backward compatible)
pub fn create_pdf_with_options(
    filename: &str,
    text: &str,
    font: &str,
    font_size: f32,
) -> Result<()> {
    let elements: Vec<Element> = text
        .lines()
        .map(|l| {
            if l.trim().is_empty() {
                Element::EmptyLine
            } else {
                Element::Paragraph {
                    text: l.to_string(),
                }
            }
        })
        .collect();
    create_pdf_from_elements(filename, &elements, font, font_size)
}

/// Rich element-based pipeline with header sizes, page numbers, etc.
pub fn create_pdf_from_elements(
    filename: &str,
    elements: &[Element],
    font: &str,
    base_font_size: f32,
) -> Result<()> {
    create_pdf_from_elements_with_layout(filename, elements, font, base_font_size, PageLayout::portrait())
}

/// Rich element-based pipeline with configurable page layout (orientation)
pub fn create_pdf_from_elements_with_layout(
    filename: &str,
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
) -> Result<()> {
    let show_page_numbers = true;
    let mut builder = ContentStreamBuilder::new(base_font_size, show_page_numbers, layout);

    for elem in elements {
        match elem {
            Element::Heading { level, text } => {
                let fs = heading_font_size(*level, base_font_size);
                builder.emit_empty_line();
                builder.emit_line(text, fs);
                builder.emit_empty_line();
            }
            Element::Paragraph { text } => {
                builder.emit_line(text, base_font_size);
            }
            Element::UnorderedListItem { text, depth } => {
                let indent = "  ".repeat(*depth as usize);
                let line = format!("{}• {}", indent, text);
                builder.emit_line(&line, base_font_size);
            }
            Element::OrderedListItem {
                number,
                text,
                depth,
            } => {
                let indent = "  ".repeat(*depth as usize);
                let line = format!("{}{}. {}", indent, number, text);
                builder.emit_line(&line, base_font_size);
            }
            Element::TaskListItem { checked, text } => {
                let marker = if *checked { "[x]" } else { "[ ]" };
                let line = format!("{} {}", marker, text);
                builder.emit_line(&line, base_font_size);
            }
            Element::CodeBlock { code, .. } => {
                let code_size = base_font_size * 0.85;
                builder.emit_empty_line();
                for code_line in code.lines() {
                    builder.emit_line(code_line, code_size);
                }
                builder.emit_empty_line();
            }
            Element::TableRow {
                cells,
                is_separator,
                alignments: _,
            } => {
                if *is_separator {
                    let sep: Vec<String> = cells.iter().map(|c| "-".repeat(c.len().max(4))).collect();
                    builder.emit_line(&sep.join("  "), base_font_size);
                } else {
                    builder.emit_line(&cells.join("  "), base_font_size);
                }
            }
            Element::DefinitionItem { term, definition } => {
                builder.emit_line(term, base_font_size);
                builder.emit_line(&format!("  {}", definition), base_font_size);
            }
            Element::Footnote { label, text } => {
                let footnote_size = base_font_size * 0.85;
                builder.emit_line(&format!("[{}] {}", label, text), footnote_size);
            }
            Element::BlockQuote { text, depth } => {
                let prefix = "> ".repeat(*depth as usize);
                builder.emit_line(&format!("{}{}", prefix, text), base_font_size);
            }
            Element::HorizontalRule => {
                builder.emit_horizontal_rule();
            }
            Element::EmptyLine => {
                builder.emit_empty_line();
            }
        }
    }

    let page_streams = builder.finish();
    assemble_pdf(filename, &page_streams, font, &layout)?;
    Ok(())
}

/// Assemble final PDF from per-page content streams
fn assemble_pdf(filename: &str, page_streams: &[Vec<u8>], font: &str, layout: &PageLayout) -> Result<()> {
    let mut generator = PdfGenerator::new();

    let mut page_ids = Vec::new();

    // We need to know the pages object ID ahead of time.
    // Layout: for each page: content_stream_obj, page_obj, font_obj
    // Then: pages_obj, catalog_obj
    // pages_obj id = page_streams.len() * 3 + 1
    let pages_obj_id = (page_streams.len() as u32) * 3 + 1;

    for page_stream in page_streams {
        let content_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", page_stream.len()),
            page_stream.clone(),
        );

        let font_id = content_id + 2; // font obj comes after page obj

        let page_dict = format!(
            "<< /Type /Page\n\
             /Parent {} 0 R\n\
             /MediaBox [0 0 {} {}]\n\
             /Contents {} 0 R\n\
             /Resources << /Font << /F1 {} 0 R >> >>\n\
             >>\n",
            pages_obj_id, layout.width, layout.height, content_id, font_id
        );
        let page_id = generator.add_object(page_dict);
        page_ids.push(page_id);

        let font_dict = format!(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
            font
        );
        generator.add_object(font_dict);
    }

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let pages_dict = format!(
        "<< /Type /Pages\n\
         /Kids [{}]\n\
         /Count {}\n\
         >>\n",
        kids.join(" "),
        page_ids.len()
    );
    let actual_pages_id = generator.add_object(pages_dict);
    assert_eq!(actual_pages_id, pages_obj_id);

    let catalog_dict = format!(
        "<< /Type /Catalog\n\
         /Pages {} 0 R\n\
         >>\n",
        actual_pages_id
    );
    generator.add_object(catalog_dict);

    let pdf_data = generator.generate();
    let mut file = File::create(filename)?;
    file.write_all(&pdf_data)?;
    Ok(())
}

fn escape_pdf_string(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}
