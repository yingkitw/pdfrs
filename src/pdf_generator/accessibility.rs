//! Accessibility / tagged PDF structure types.
//!
//! Used by [`generate_tagged_pdf_bytes`](super::generate_tagged_pdf_bytes) and
//! structure-tree helpers.

use super::{escape_pdf_string, render_math_text};
use crate::elements::{Element, TextSegment};

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
        let mut dict = format!(
            "<< /Type /StructElem /S /{}",
            self.struct_type.as_pdf_name()
        );

        if let Some(ref alt) = self.alt_text {
            dict.push_str(&format!(" /Alt {}", escape_pdf_string(alt)));
        }

        if let Some(ref actual) = self.actual_text {
            dict.push_str(&format!(" /A {}", escape_pdf_string(actual)));
        }

        if let Some(ref content_id) = self.content_id {
            dict.push_str(&format!(" /K {} 0 R", content_id));
        } else if !self.children.is_empty() {
            let kid_refs: Vec<String> = self
                .children
                .iter()
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
            StructureElement::new(struct_type).with_actual_text(text.clone())
        }
        Element::Paragraph { text } => {
            StructureElement::new(StructureType::P).with_actual_text(text.clone())
        }
        Element::RichParagraph { segments } => {
            let text = segments
                .iter()
                .map(|s| match s {
                    TextSegment::Plain(t)
                    | TextSegment::Bold(t)
                    | TextSegment::Italic(t)
                    | TextSegment::BoldItalic(t)
                    | TextSegment::Strikethrough(t) => t.clone(),
                    TextSegment::Code(c) => format!("`{}`", c),
                    TextSegment::MathInline(expr) => render_math_text(expr),
                    TextSegment::Link { text, url } => format!("{} ({})", text, url),
                    TextSegment::Citation { key } => format!("[@{}]", key),
                })
                .collect::<Vec<_>>()
                .join("");
            StructureElement::new(StructureType::P).with_actual_text(text)
        }
        Element::UnorderedListItem { text, .. }
        | Element::OrderedListItem { text, .. }
        | Element::TaskListItem { text, .. } => {
            StructureElement::new(StructureType::LI).with_actual_text(text.clone())
        }
        Element::CodeBlock { code, .. } => {
            StructureElement::new(StructureType::Code).with_actual_text(code.clone())
        }
        Element::BlockQuote { text, .. } => {
            StructureElement::new(StructureType::BlockQuote).with_actual_text(text.clone())
        }
        Element::TableRow { .. } => StructureElement::new(StructureType::TR),
        Element::HorizontalRule => StructureElement::new(StructureType::NonStruct),
        Element::EmptyLine => StructureElement::new(StructureType::NonStruct),
        Element::Columns { .. } => StructureElement::new(StructureType::NonStruct),
        Element::PageNumberMode { .. }
        | Element::RunningHeaderMode { .. }
        | Element::Toc
        | Element::Bibliography
        | Element::CitationDef { .. } => StructureElement::new(StructureType::NonStruct),
        Element::Chart { title, .. } => StructureElement::new(StructureType::Figure)
            .with_alt_text(title.clone().unwrap_or_else(|| "Chart".into())),
        Element::Footnote { .. } => StructureElement::new(StructureType::Note),
        Element::DefinitionItem { .. } => StructureElement::new(StructureType::Div),
        Element::InlineCode { code } => {
            StructureElement::new(StructureType::Code).with_actual_text(code.clone())
        }
        Element::Link { text, url } => StructureElement::new(StructureType::Link)
            .with_actual_text(format!("{} ({})", text, url)),
        Element::Image { alt, .. } => {
            StructureElement::new(StructureType::Figure).with_alt_text(alt.clone())
        }
        Element::StyledText { text, .. } => {
            StructureElement::new(StructureType::Span).with_actual_text(text.clone())
        }
        Element::MathBlock { expression } => {
            StructureElement::new(StructureType::Formula).with_actual_text(expression.clone())
        }
        Element::MathInline { expression } => {
            StructureElement::new(StructureType::Formula).with_actual_text(expression.clone())
        }
        Element::PageBreak => StructureElement::new(StructureType::NonStruct),
    }
}
