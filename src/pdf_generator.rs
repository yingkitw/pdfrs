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

/// RGB color for text rendering (0.0-1.0 per channel)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub fn black() -> Self { Color { r: 0.0, g: 0.0, b: 0.0 } }
    pub fn red() -> Self { Color { r: 1.0, g: 0.0, b: 0.0 } }
    pub fn blue() -> Self { Color { r: 0.0, g: 0.0, b: 1.0 } }
    pub fn gray() -> Self { Color { r: 0.5, g: 0.5, b: 0.5 } }
    pub fn rgb(r: f32, g: f32, b: f32) -> Self { Color { r, g, b } }
}

/// Text alignment for line rendering
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
}

struct ContentStreamBuilder {
    pages: Vec<Vec<u8>>,
    current: Vec<u8>,
    y: f32,
    base_font_size: f32,
    current_font_size: f32,
    current_color: Color,
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
            current_color: Color::black(),
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
    }

    fn set_font(&mut self, size: f32) {
        // Always set font after Td to ensure font state is current
        self.current_font_size = size;
        self.current
            .extend_from_slice(format!("/F1 {} Tf\n", size).as_bytes());
    }

    fn set_color(&mut self, color: Color) {
        if self.current_color != color {
            self.current_color = color;
            self.current
                .extend_from_slice(format!("{} {} {} rg\n", color.r, color.g, color.b).as_bytes());
        }
    }

    fn reset_color(&mut self) {
        self.set_color(Color::black());
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
            .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, y).as_bytes());
        self.current
            .extend_from_slice(format!("({}) Tj\n", escape_pdf_string(&label)).as_bytes());
        self.current.extend_from_slice(b"ET\n");
    }

    fn emit_line(&mut self, text: &str, font_size: f32) {
        self.emit_line_aligned(text, font_size, TextAlign::Left);
    }

    fn emit_line_aligned(&mut self, text: &str, font_size: f32, align: TextAlign) {
        let lh = line_height(font_size);
        if self.needs_page_break(lh) {
            self.new_page();
        }
        self.set_font(font_size);
        let escaped = escape_pdf_string(text);

        let x = match align {
            TextAlign::Left => self.layout.margin_left,
            TextAlign::Center => {
                // Approximate: 0.5 * char_count * font_size * 0.5
                let approx_width = text.len() as f32 * font_size * 0.5;
                self.layout.margin_left + (self.layout.content_width() - approx_width) / 2.0
            }
        };

        // Use Tm (text matrix) for absolute positioning — Td is relative and compounds
        self.current
            .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, self.y).as_bytes());
        self.current
            .extend_from_slice(format!("({}) Tj\n", escaped).as_bytes());
        self.y -= lh;
    }

    fn emit_colored_line(&mut self, text: &str, font_size: f32, color: Color) {
        self.set_color(color);
        self.emit_line(text, font_size);
        self.reset_color();
    }

    fn emit_empty_line(&mut self) {
        let lh = line_height(self.base_font_size) * 0.5;
        if self.needs_page_break(lh) {
            self.new_page();
        }
        self.y -= lh;
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
    render_elements_to_builder(&mut builder, elements, base_font_size);
    let page_streams = builder.finish();
    assemble_pdf(filename, &page_streams, font, &layout)?;
    Ok(())
}

/// Render elements into a ContentStreamBuilder (shared by file and bytes APIs)
fn render_elements_to_builder(builder: &mut ContentStreamBuilder, elements: &[Element], base_font_size: f32) {
    for elem in elements {
        match elem {
            Element::Heading { level, text } => {
                let fs = heading_font_size(*level, base_font_size);
                let align = if *level == 1 { TextAlign::Center } else { TextAlign::Left };
                builder.emit_empty_line();
                builder.emit_line_aligned(text, fs, align);
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
                builder.set_color(Color::gray());
                for code_line in code.lines() {
                    builder.emit_line(code_line, code_size);
                }
                builder.reset_color();
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
            Element::InlineCode { code } => {
                let code_size = base_font_size * 0.9;
                builder.set_color(Color::gray());
                builder.emit_line(code, code_size);
                builder.reset_color();
            }
            Element::Link { text, url } => {
                builder.set_color(Color::blue());
                builder.emit_line(&format!("{} ({})", text, url), base_font_size);
                builder.reset_color();
            }
            Element::Image { alt, path } => {
                builder.emit_line(&format!("[Image: {}] ({})", alt, path), base_font_size);
            }
            Element::StyledText { text, bold, italic } => {
                let prefix = match (*bold, *italic) {
                    (true, true) => "***",
                    (true, false) => "**",
                    (false, true) => "*",
                    _ => "",
                };
                let suffix = prefix;
                builder.emit_line(&format!("{}{}{}", prefix, text, suffix), base_font_size);
            }
            Element::PageBreak => {
                builder.new_page();
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
}

/// Generate PDF bytes from elements (library API — no filesystem access needed)
pub fn generate_pdf_bytes(
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
) -> Result<Vec<u8>> {
    let show_page_numbers = true;
    let mut builder = ContentStreamBuilder::new(base_font_size, show_page_numbers, layout);
    render_elements_to_builder(&mut builder, elements, base_font_size);
    let page_streams = builder.finish();
    Ok(assemble_pdf_bytes(&page_streams, font, &layout))
}

/// Assemble final PDF bytes from per-page content streams
fn assemble_pdf_bytes(page_streams: &[Vec<u8>], font: &str, layout: &PageLayout) -> Vec<u8> {
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

    generator.generate()
}

/// Assemble final PDF from per-page content streams and write to file
fn assemble_pdf(filename: &str, page_streams: &[Vec<u8>], font: &str, layout: &PageLayout) -> Result<()> {
    let pdf_data = assemble_pdf_bytes(page_streams, font, layout);
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

// --- Accessibility / Tagged PDF support ---

/// Accessibility options for PDF generation
#[derive(Debug, Clone)]
pub struct AccessibilityOptions {
    /// Enable tagged PDF (PDF/UA compliance)
    pub tagged_pdf: bool,
    /// Document language (e.g., "en-US", "en-GB")
    pub language: String,
    /// Document title for accessibility
    pub title: Option<String>,
}

impl Default for AccessibilityOptions {
    fn default() -> Self {
        Self {
            tagged_pdf: false,
            language: "en".to_string(),
            title: None,
        }
    }
}

impl AccessibilityOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tagged_pdf(mut self, tagged: bool) -> Self {
        self.tagged_pdf = tagged;
        self
    }

    pub fn with_language(mut self, lang: String) -> Self {
        self.language = lang;
        self
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }
}

/// Structure element types for tagged PDF
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureType {
    Document,
    Part,
    Art,
    Sect,
    Div,
    BlockQuote,
    Caption,
    TOC,
    TOCI,
    Index,
    NonStruct,
    Private,
    P,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    L,
    LI,
    Lbl,
    LBody,
    Table,
    TR,
    TH,
    TD,
    THead,
    TBody,
    TFoot,
    Span,
    Quote,
    Note,
    Reference,
    BibEntry,
    Code,
    Link,
    Figure,
    Formula,
}

impl StructureType {
    /// Get the PDF structure type name as per PDF 1.7 specification
    pub fn as_pdf_name(&self) -> &str {
        match self {
            Self::Document => "Document",
            Self::Part => "Part",
            Self::Art => "Art",
            Self::Sect => "Sect",
            Self::Div => "Div",
            Self::BlockQuote => "BlockQuote",
            Self::Caption => "Caption",
            Self::TOC => "TOC",
            Self::TOCI => "TOCI",
            Self::Index => "Index",
            Self::NonStruct => "NonStruct",
            Self::Private => "Private",
            Self::P => "P",
            Self::H1 => "H1",
            Self::H2 => "H2",
            Self::H3 => "H3",
            Self::H4 => "H4",
            Self::H5 => "H5",
            Self::H6 => "H6",
            Self::L => "L",
            Self::LI => "LI",
            Self::Lbl => "Lbl",
            Self::LBody => "LBody",
            Self::Table => "Table",
            Self::TR => "TR",
            Self::TH => "TH",
            Self::TD => "TD",
            Self::THead => "THead",
            Self::TBody => "TBody",
            Self::TFoot => "TFoot",
            Self::Span => "Span",
            Self::Quote => "Quote",
            Self::Note => "Note",
            Self::Reference => "Reference",
            Self::BibEntry => "BibEntry",
            Self::Code => "Code",
            Self::Link => "Link",
            Self::Figure => "Figure",
            Self::Formula => "Formula",
        }
    }
}

/// Structure element for tagged PDF
#[derive(Debug, Clone)]
pub struct StructureElement {
    pub struct_type: StructureType,
    pub alt_text: Option<String>,
    pub actual_text: Option<String>,
    pub children: Vec<StructureElement>,
    pub content_id: Option<u32>, // Reference to content object
}

impl StructureElement {
    pub fn new(struct_type: StructureType) -> Self {
        Self {
            struct_type,
            alt_text: None,
            actual_text: None,
            children: Vec::new(),
            content_id: None,
        }
    }

    pub fn with_alt_text(mut self, text: String) -> Self {
        self.alt_text = Some(text);
        self
    }

    pub fn with_actual_text(mut self, text: String) -> Self {
        self.actual_text = Some(text);
        self
    }

    pub fn with_children(mut self, children: Vec<StructureElement>) -> Self {
        self.children = children;
        self
    }

    pub fn add_child(&mut self, child: StructureElement) {
        self.children.push(child);
    }

    pub fn with_content_id(mut self, id: u32) -> Self {
        self.content_id = Some(id);
        self
    }

    /// Generate the structure element dictionary for PDF
    pub fn to_pdf_dict(&self, obj_id: u32) -> String {
        let mut dict = format!("<< /Type /StructElem /S /{}", self.struct_type.as_pdf_name());

        if let Some(ref alt) = self.alt_text {
            dict.push_str(&format!(" /Alt {}", escape_pdf_string(alt)));
        }

        if let Some(ref actual) = self.actual_text {
            dict.push_str(&format!(" /A {}", escape_pdf_string(actual)));
        }

        if let Some(ref content_id) = self.content_id {
            dict.push_str(&format!(" /K {} 0 R", content_id));
        } else if !self.children.is_empty() {
            let kid_refs: Vec<String> = self.children.iter()
                .enumerate()
                .map(|(i, _)| format!("{} 0 R", obj_id + 1 + i as u32))
                .collect();
            dict.push_str(&format!(" /K [{}]", kid_refs.join(" ")));
        } else {
            dict.push_str(" /K 0"); // No content
        }

        dict.push_str(" >>");
        dict
    }
}

/// Convert Element to StructureElement for accessibility
pub fn element_to_structure(element: &Element) -> StructureElement {
    match element {
        Element::Heading { level, text } => {
            let struct_type = match level {
                1 => StructureType::H1,
                2 => StructureType::H2,
                3 => StructureType::H3,
                4 => StructureType::H4,
                5 => StructureType::H5,
                _ => StructureType::H6,
            };
            StructureElement::new(struct_type)
                .with_actual_text(text.clone())
        }
        Element::Paragraph { text } => {
            StructureElement::new(StructureType::P)
                .with_actual_text(text.clone())
        }
        Element::UnorderedListItem { text, .. } | Element::OrderedListItem { text, .. } | Element::TaskListItem { text, .. } => {
            StructureElement::new(StructureType::LI)
                .with_actual_text(text.clone())
        }
        Element::CodeBlock { code, .. } => {
            StructureElement::new(StructureType::Code)
                .with_actual_text(code.clone())
        }
        Element::BlockQuote { text, .. } => {
            StructureElement::new(StructureType::BlockQuote)
                .with_actual_text(text.clone())
        }
        Element::TableRow { .. } => {
            StructureElement::new(StructureType::TR)
        }
        Element::HorizontalRule => {
            StructureElement::new(StructureType::NonStruct)
        }
        Element::EmptyLine => {
            StructureElement::new(StructureType::NonStruct)
        }
        Element::Footnote { .. } => {
            StructureElement::new(StructureType::Note)
        }
        Element::DefinitionItem { .. } => {
            StructureElement::new(StructureType::Div)
        }
        Element::InlineCode { code } => {
            StructureElement::new(StructureType::Code)
                .with_actual_text(code.clone())
        }
        Element::Link { text, url } => {
            StructureElement::new(StructureType::Link)
                .with_actual_text(format!("{} ({})", text, url))
        }
        Element::Image { alt, .. } => {
            StructureElement::new(StructureType::Figure)
                .with_alt_text(alt.clone())
        }
        Element::StyledText { text, .. } => {
            StructureElement::new(StructureType::Span)
                .with_actual_text(text.clone())
        }
        Element::PageBreak => {
            StructureElement::new(StructureType::NonStruct)
        }
    }
}

#[cfg(test)]
mod accessibility_tests {
    use super::*;

    #[test]
    fn test_accessibility_options_default() {
        let opts = AccessibilityOptions::default();
        assert!(!opts.tagged_pdf);
        assert_eq!(opts.language, "en");
        assert!(opts.title.is_none());
    }

    #[test]
    fn test_accessibility_options_builder() {
        let opts = AccessibilityOptions::new()
            .with_tagged_pdf(true)
            .with_language("en-US".to_string())
            .with_title("My Document".to_string());

        assert!(opts.tagged_pdf);
        assert_eq!(opts.language, "en-US");
        assert_eq!(opts.title, Some("My Document".to_string()));
    }

    #[test]
    fn test_structure_type_names() {
        assert_eq!(StructureType::Document.as_pdf_name(), "Document");
        assert_eq!(StructureType::P.as_pdf_name(), "P");
        assert_eq!(StructureType::H1.as_pdf_name(), "H1");
        assert_eq!(StructureType::Figure.as_pdf_name(), "Figure");
    }

    #[test]
    fn test_structure_element_builder() {
        let elem = StructureElement::new(StructureType::P)
            .with_alt_text("A paragraph".to_string())
            .with_actual_text("This is the actual text".to_string());

        assert_eq!(elem.struct_type, StructureType::P);
        assert_eq!(elem.alt_text, Some("A paragraph".to_string()));
        assert_eq!(elem.actual_text, Some("This is the actual text".to_string()));
    }

    #[test]
    fn test_structure_element_with_children() {
        let mut parent = StructureElement::new(StructureType::L);
        parent.add_child(StructureElement::new(StructureType::LI));
        parent.add_child(StructureElement::new(StructureType::LI));

        assert_eq!(parent.children.len(), 2);
    }

    #[test]
    fn test_element_to_structure_heading() {
        let elem = Element::Heading { level: 1, text: "Hello".into() };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::H1);
        assert_eq!(struct_elem.actual_text, Some("Hello".to_string()));
    }

    #[test]
    fn test_element_to_structure_paragraph() {
        let elem = Element::Paragraph { text: "Test paragraph".into() };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::P);
        assert_eq!(struct_elem.actual_text, Some("Test paragraph".to_string()));
    }

    #[test]
    fn test_element_to_structure_code() {
        let elem = Element::CodeBlock { language: "rust".into(), code: "fn main() {}".into() };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::Code);
        assert_eq!(struct_elem.actual_text, Some("fn main() {}".to_string()));
    }
}
