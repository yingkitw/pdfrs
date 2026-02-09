use crate::elements::{self, Element, TextSegment};
use anyhow::Result;
use std::fs::File;
use std::io::Read;

/// Convert markdown to plain text (legacy, kept for backward compat / unit tests)
pub fn markdown_to_text(markdown: &str) -> String {
    let elements = elements::parse_markdown(markdown);
    elements_to_text(&elements)
}

/// Render structured elements back to plain text
fn elements_to_text(elements: &[Element]) -> String {
    let mut text = String::new();
    for elem in elements {
        match elem {
            Element::Heading { text: t, .. } => {
                text.push_str(t);
                text.push('\n');
            }
            Element::Paragraph { text: t } => {
                text.push_str(t);
                text.push('\n');
            }
            Element::RichParagraph { segments } => {
                for segment in segments {
                    match segment {
                        TextSegment::Plain(t) | TextSegment::Bold(t) | TextSegment::Italic(t) | TextSegment::BoldItalic(t) => {
                            text.push_str(t);
                        }
                        TextSegment::Code(c) => {
                            text.push('`');
                            text.push_str(c);
                            text.push('`');
                        }
                        TextSegment::Link { text: t, url } => {
                            text.push('[');
                            text.push_str(t);
                            text.push_str("](");
                            text.push_str(url);
                            text.push_str(")");
                        }
                    }
                }
                text.push('\n');
            }
            Element::UnorderedListItem { text: t, .. } => {
                text.push_str("• ");
                text.push_str(t);
                text.push('\n');
            }
            Element::OrderedListItem { number: _, text: t, .. } => {
                text.push_str("• ");
                text.push_str(t);
                text.push('\n');
            }
            Element::TaskListItem { checked, text: t } => {
                if *checked {
                    text.push_str("[x] ");
                } else {
                    text.push_str("[ ] ");
                }
                text.push_str(t);
                text.push('\n');
            }
            Element::CodeBlock { code, .. } => {
                text.push('\n');
                text.push_str(code);
                text.push_str("\n\n");
            }
            Element::TableRow { cells, is_separator, alignments: _ } => {
                if *is_separator {
                    let sep: Vec<String> = cells.iter().map(|c| "-".repeat(c.len().max(4))).collect();
                    text.push_str(&sep.join("  "));
                } else {
                    text.push_str(&cells.join("  "));
                }
                text.push_str("  \n");
            }
            Element::DefinitionItem { term, definition } => {
                text.push_str(term);
                text.push_str(": ");
                text.push_str(definition);
                text.push('\n');
            }
            Element::Footnote { label, text: t } => {
                text.push_str(&format!("[{}] {}", label, t));
                text.push('\n');
            }
            Element::BlockQuote { text: t, depth } => {
                let prefix = "> ".repeat(*depth as usize);
                text.push_str(&prefix);
                text.push_str(t);
                text.push('\n');
            }
            Element::InlineCode { code } => {
                text.push_str(code);
                text.push('\n');
            }
            Element::Link { text: t, url } => {
                text.push_str(t);
                text.push_str(" (");
                text.push_str(url);
                text.push_str(")\n");
            }
            Element::Image { alt, path } => {
                text.push_str("[Image: ");
                text.push_str(alt);
                text.push_str("] (");
                text.push_str(path);
                text.push_str(")\n");
            }
            Element::StyledText { text: t, .. } => {
                text.push_str(t);
                text.push('\n');
            }
            Element::MathBlock { expression } => {
                text.push_str("$$\n");
                text.push_str(expression);
                text.push_str("\n$$\n");
            }
            Element::MathInline { expression } => {
                text.push('$');
                text.push_str(expression);
                text.push_str("$\n");
            }
            Element::PageBreak => {
                text.push_str("\n---\n");
            }
            Element::HorizontalRule => {
                text.push_str("---\n");
            }
            Element::EmptyLine => {}
        }
    }
    text
}

pub fn markdown_to_pdf(markdown_file: &str, pdf_file: &str) -> Result<()> {
    markdown_to_pdf_with_options(markdown_file, pdf_file, "Helvetica", 12.0)
}

pub fn markdown_to_pdf_with_options(
    markdown_file: &str,
    pdf_file: &str,
    font: &str,
    font_size: f32,
) -> Result<()> {
    markdown_to_pdf_full(
        markdown_file,
        pdf_file,
        font,
        font_size,
        crate::pdf_generator::PageOrientation::Portrait,
    )
}

pub fn markdown_to_pdf_full(
    markdown_file: &str,
    pdf_file: &str,
    font: &str,
    font_size: f32,
    orientation: crate::pdf_generator::PageOrientation,
) -> Result<()> {
    let mut file = File::open(markdown_file)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let elements = elements::parse_markdown(&content);
    let layout = crate::pdf_generator::PageLayout::from_orientation(orientation);
    crate::pdf_generator::create_pdf_from_elements_with_layout(
        pdf_file, &elements, font, font_size, layout,
    )?;

    Ok(())
}
