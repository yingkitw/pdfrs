//! Markdown to PDF conversion utilities
//!
//! High-level helpers like [`markdown_to_pdf_full`] that read a Markdown file
//! and write a complete PDF, plus [`watch_markdown_to_pdf`] for hot-reload
//! development workflows.

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
                        TextSegment::Plain(t)
                        | TextSegment::Bold(t)
                        | TextSegment::Italic(t)
                        | TextSegment::BoldItalic(t)
                        | TextSegment::Strikethrough(t) => {
                            text.push_str(t);
                        }
                        TextSegment::Code(c) => {
                            text.push('`');
                            text.push_str(c);
                            text.push('`');
                        }
                        TextSegment::MathInline(expr) => {
                            text.push('$');
                            text.push_str(expr);
                            text.push('$');
                        }
                        TextSegment::Link { text: t, url } => {
                            text.push('[');
                            text.push_str(t);
                            text.push_str("](");
                            text.push_str(url);
                            text.push(')');
                        }
                        TextSegment::Citation { key } => {
                            text.push_str("[@");
                            text.push_str(key);
                            text.push(']');
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
            Element::Chart { kind, title, points } => {
                text.push_str("[Chart ");
                text.push_str(match kind {
                    crate::elements::ChartKind::Bar => "bar",
                    crate::elements::ChartKind::Line => "line",
                    crate::elements::ChartKind::Pie => "pie",
                });
                if let Some(t) = title {
                    text.push_str(": ");
                    text.push_str(t);
                }
                text.push_str("] ");
                for (i, (label, value)) in points.iter().enumerate() {
                    if i > 0 {
                        text.push_str(", ");
                    }
                    text.push_str(label);
                    text.push('=');
                    text.push_str(&value.to_string());
                }
                text.push('\n');
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
            Element::Columns { count } => {
                text.push_str(&format!("\n<!-- columns:{} -->\n", count));
            }
            Element::PageNumberMode { style } => {
                let s = match style {
                    crate::elements::PageNumberStyle::Roman => "roman",
                    crate::elements::PageNumberStyle::Arabic => "arabic",
                    crate::elements::PageNumberStyle::None => "none",
                };
                text.push_str(&format!("\n<!-- pagenumber:{} -->\n", s));
            }
            Element::RunningHeaderMode { enabled } => {
                text.push_str(&format!(
                    "\n<!-- running-header:{} -->\n",
                    if *enabled { "on" } else { "off" }
                ));
            }
            Element::Toc => text.push_str("\n<!-- toc -->\n"),
            Element::Bibliography => text.push_str("\n<!-- bibliography -->\n"),
            Element::CitationDef { key, text: t } => {
                text.push_str(&format!("[@{}]: {}\n", key, t));
            }
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
    let image_base = std::path::Path::new(markdown_file)
        .parent()
        .map(|p| p.to_path_buf());
    let bytes = crate::pdf_generator::generate_pdf_bytes_internal_with_base(
        &elements,
        font,
        font_size,
        layout,
        None,
        false,
        None,
        image_base,
    )?;
    std::fs::write(pdf_file, bytes)?;
    Ok(())
}

/// Watch a markdown file for changes and regenerate the PDF automatically.
///
/// Polls the file's modification time every `interval_ms` milliseconds (default 1000).
/// Whenever the file changes, it is re-parsed and a new PDF is generated.
/// Press Ctrl+C to stop watching.
///
/// # Example
/// ```no_run
/// use pdfrs::markdown::watch_markdown_to_pdf;
/// use pdfrs::pdf_generator::PageOrientation;
///
/// watch_markdown_to_pdf("doc.md", "doc.pdf", "Helvetica", 12.0, PageOrientation::Portrait, Some(500)).unwrap();
/// ```
pub fn watch_markdown_to_pdf(
    markdown_file: &str,
    pdf_file: &str,
    font: &str,
    font_size: f32,
    orientation: crate::pdf_generator::PageOrientation,
    interval_ms: Option<u64>,
) -> Result<()> {
    use std::time::Duration;
    use std::thread;

    let interval = Duration::from_millis(interval_ms.unwrap_or(1000));
    let mut last_modified = std::fs::metadata(markdown_file)?.modified()?;

    // Generate initial PDF
    println!("[watch] Generating initial PDF from {}", markdown_file);
    markdown_to_pdf_full(markdown_file, pdf_file, font, font_size, orientation)?;
    println!("[watch] Watching {} for changes (interval: {:?}). Press Ctrl+C to stop.", markdown_file, interval);

    loop {
        thread::sleep(interval);

        let metadata = match std::fs::metadata(markdown_file) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[watch] Error reading file metadata: {}", e);
                continue;
            }
        };

        let current_modified = match metadata.modified() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[watch] Error reading modification time: {}", e);
                continue;
            }
        };

        if current_modified > last_modified {
            last_modified = current_modified;
            println!("[watch] Change detected at {:?}, regenerating PDF...", current_modified);
            match markdown_to_pdf_full(markdown_file, pdf_file, font, font_size, orientation) {
                Ok(_) => println!("[watch] PDF updated: {}", pdf_file),
                Err(e) => eprintln!("[watch] Error regenerating PDF: {}", e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_markdown_to_pdf_full_roundtrip() {
        let tmp_dir = std::env::temp_dir();
        let md_path = tmp_dir.join("test_watch.md");
        let pdf_path = tmp_dir.join("test_watch.pdf");

        // Write initial markdown
        {
            let mut f = std::fs::File::create(&md_path).unwrap();
            f.write_all(b"# Hello\n\nWorld.").unwrap();
        }

        // Generate PDF
        markdown_to_pdf_full(
            &md_path.to_string_lossy(),
            &pdf_path.to_string_lossy(),
            "Helvetica",
            12.0,
            crate::pdf_generator::PageOrientation::Portrait,
        ).unwrap();

        assert!(pdf_path.exists(), "PDF should be created");
        let bytes1 = std::fs::read(&pdf_path).unwrap();
        assert!(!bytes1.is_empty(), "PDF should not be empty");

        // Modify markdown
        std::thread::sleep(std::time::Duration::from_millis(50));
        {
            let mut f = std::fs::File::create(&md_path).unwrap();
            f.write_all(b"# Updated\n\nNew content here.").unwrap();
        }

        // Regenerate PDF
        markdown_to_pdf_full(
            &md_path.to_string_lossy(),
            &pdf_path.to_string_lossy(),
            "Helvetica",
            12.0,
            crate::pdf_generator::PageOrientation::Portrait,
        ).unwrap();

        let bytes2 = std::fs::read(&pdf_path).unwrap();
        assert!(!bytes2.is_empty(), "Regenerated PDF should not be empty");

        // PDF content should reflect the update
        let content = String::from_utf8_lossy(&bytes2);
        assert!(content.contains("Updated") || content.contains("New content"),
            "Updated PDF should contain new content");

        // Cleanup
        let _ = std::fs::remove_file(&md_path);
        let _ = std::fs::remove_file(&pdf_path);
    }
}
