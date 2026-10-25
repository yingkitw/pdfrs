//! HTML-to-PDF conversion via the existing Element pipeline.
//!
//! This module provides a lightweight HTML parser that converts common HTML
//! documents into `Element` vectors, which are then rendered to PDF using
//! the existing `crate::pdf_generator` pipeline.
//!
//! # CSS support
//!
//! The parser extracts CSS rules from `<style>` tags and inline `style`
//! attributes. Supported properties: `font-weight`, `font-style`,
//! `text-align`, `color`, `background-color`, `font-size`, `margin`,
//! `padding`, `border`. Selectors: tag, `.class`, `tag.class`, `#id`.
//! Inline styles take priority over stylesheet rules.
//!
//! # Supported HTML elements
//!
//! | HTML | Element mapping |
//! |-----|-----------------|
//! | `<h1>`–`<h6>` | `Element::Heading` |
//! | `<p>` | `Element::Paragraph` / `Element::RichParagraph` |
//! | `<strong>`, `<b>` | Bold `TextSegment` |
//! | `<em>`, `<i>` | Italic `TextSegment` |
//! | `<code>` | `TextSegment::Code` |
//! | `<pre><code>` | `Element::CodeBlock` |
//! | `<ul><li>` | `Element::UnorderedListItem` |
//! | `<ol><li>` | `Element::OrderedListItem` |
//! | `<table>`, `<tr>`, `<th>`, `<td>` | `Element::TableRow` |
//! | `<blockquote>` | `Element::BlockQuote` |
//! | `<img>` | `Element::Image` |
//! | `<a href>` | `TextSegment::Link` |
//! | `<hr>` | `Element::HorizontalRule` |
//! | `<br>` | Line break within paragraph |
//!
//! # Example
//!
//! ```rust,no_run
//! use pdfrs::html;
//!
//! let html = "<h1>Title</h1><p>Hello <strong>world</strong></p>";
//! let elements = html::parse_html(html);
//! assert!(!elements.is_empty());
//! ```

mod convert;
mod css;
mod tokenizer;

pub use convert::{html_to_pdf, html_to_pdf_bytes, parse_html};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements::{Element, TableAlignment, TextSegment};

    #[test]
    fn test_parse_heading() {
        let elements = parse_html("<h1>Title</h1>");
        assert_eq!(elements.len(), 1);
        assert!(matches!(&elements[0], Element::Heading { level: 1, text } if text == "Title"));
    }

    #[test]
    fn test_parse_heading_levels() {
        let elements = parse_html("<h2>Sub</h2><h3>SubSub</h3>");
        assert_eq!(elements.len(), 2);
        assert!(matches!(&elements[0], Element::Heading { level: 2, .. }));
        assert!(matches!(&elements[1], Element::Heading { level: 3, .. }));
    }

    #[test]
    fn test_parse_paragraph() {
        let elements = parse_html("<p>Hello world</p>");
        assert_eq!(elements.len(), 1);
        assert!(matches!(&elements[0], Element::Paragraph { text } if text == "Hello world"));
    }

    #[test]
    fn test_parse_rich_paragraph_bold() {
        let elements = parse_html("<p>Hello <strong>world</strong></p>");
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::Bold(t) if t == "world"))
                );
            }
            _ => panic!("expected RichParagraph"),
        }
    }

    #[test]
    fn test_parse_rich_paragraph_italic() {
        let elements = parse_html("<p>Hello <em>world</em></p>");
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::Italic(t) if t == "world"))
                );
            }
            _ => panic!("expected RichParagraph"),
        }
    }

    #[test]
    fn test_parse_bold_italic() {
        let elements = parse_html("<p><strong><em>both</em></strong></p>");
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::BoldItalic(t) if t == "both"))
                );
            }
            _ => panic!("expected RichParagraph"),
        }
    }

    #[test]
    fn test_parse_inline_code() {
        let elements = parse_html("<p>Use <code>println!</code> function</p>");
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::Code(t) if t == "println!"))
                );
            }
            _ => panic!("expected RichParagraph"),
        }
    }

    #[test]
    fn test_parse_code_block() {
        let elements = parse_html("<pre><code class=\"language-rust\">fn main() {}</code></pre>");
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::CodeBlock { language, code } => {
                assert_eq!(language, "rust");
                assert_eq!(code, "fn main() {}");
            }
            _ => panic!("expected CodeBlock"),
        }
    }

    #[test]
    fn test_parse_unordered_list() {
        let elements = parse_html("<ul><li>One</li><li>Two</li></ul>");
        assert_eq!(elements.len(), 2);
        assert!(matches!(&elements[0], Element::UnorderedListItem { text, .. } if text == "One"));
        assert!(matches!(&elements[1], Element::UnorderedListItem { text, .. } if text == "Two"));
    }

    #[test]
    fn test_parse_ordered_list() {
        let elements = parse_html("<ol><li>First</li><li>Second</li></ol>");
        assert_eq!(elements.len(), 2);
        assert!(
            matches!(&elements[0], Element::OrderedListItem { number: 1, text, .. } if text == "First")
        );
        assert!(
            matches!(&elements[1], Element::OrderedListItem { number: 2, text, .. } if text == "Second")
        );
    }

    #[test]
    fn test_parse_blockquote() {
        let elements = parse_html("<blockquote>To be or not to be</blockquote>");
        assert_eq!(elements.len(), 1);
        assert!(
            matches!(&elements[0], Element::BlockQuote { text, .. } if text == "To be or not to be")
        );
    }

    #[test]
    fn test_parse_hr() {
        let elements = parse_html("<hr>");
        assert_eq!(elements.len(), 1);
        assert!(matches!(&elements[0], Element::HorizontalRule));
    }

    #[test]
    fn test_parse_img() {
        let elements = parse_html("<img src=\"photo.jpg\" alt=\"Photo\">");
        assert_eq!(elements.len(), 1);
        assert!(
            matches!(&elements[0], Element::Image { alt, path } if alt == "Photo" && path == "photo.jpg")
        );
    }

    #[test]
    fn test_parse_link() {
        let elements = parse_html("<a href=\"https://example.com\">Example</a>");
        assert_eq!(elements.len(), 1);
        assert!(
            matches!(&elements[0], Element::Link { text, url } if text == "Example" && url == "https://example.com")
        );
    }

    #[test]
    fn test_parse_inline_link() {
        let elements = parse_html("<p>Visit <a href=\"https://rust-lang.org\">Rust</a> now</p>");
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(segments.iter().any(|s| matches!(s, TextSegment::Link { text, url } if text == "Rust" && url == "https://rust-lang.org")));
            }
            _ => panic!("expected RichParagraph"),
        }
    }

    #[test]
    fn test_parse_table() {
        let html = r#"<table>
            <thead><tr><th>Name</th><th>Age</th></tr></thead>
            <tbody><tr><td>Alice</td><td>30</td></tr></tbody>
        </table>"#;
        let elements = parse_html(html);
        // Should produce: header row, separator row, data row
        assert!(elements.len() >= 2);
        assert!(elements.iter().any(|e| matches!(
            e,
            Element::TableRow {
                is_separator: true,
                ..
            }
        )));
        assert!(elements.iter().any(|e| matches!(e, Element::TableRow { cells, is_separator: false, .. } if cells.contains(&"Alice".to_string()))));
    }

    #[test]
    fn test_parse_div_recursion() {
        let elements = parse_html("<div><h1>Title</h1><p>Body</p></div>");
        assert_eq!(elements.len(), 2);
        assert!(matches!(&elements[0], Element::Heading { level: 1, .. }));
        assert!(matches!(&elements[1], Element::Paragraph { .. }));
    }

    #[test]
    fn test_parse_strikethrough() {
        let elements = parse_html("<p><del>deleted</del></p>");
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::Strikethrough(t) if t == "deleted"))
                );
            }
            _ => panic!("expected RichParagraph"),
        }
    }

    #[test]
    fn test_entity_decoding() {
        let elements = parse_html("<p>Tom &amp; Jerry &lt;3</p>");
        assert_eq!(elements.len(), 1);
        assert!(matches!(&elements[0], Element::Paragraph { text } if text == "Tom & Jerry <3"));
    }

    #[test]
    fn test_self_closing_tag() {
        let elements = parse_html("<p>Line one<br/>Line two</p>");
        assert_eq!(elements.len(), 1);
        let combined = match &elements[0] {
            Element::RichParagraph { segments } => segments
                .iter()
                .map(|s| match s {
                    TextSegment::Plain(t) => t.clone(),
                    _ => String::new(),
                })
                .collect::<String>(),
            Element::Paragraph { text } => text.clone(),
            _ => panic!("expected Paragraph or RichParagraph"),
        };
        assert!(combined.contains("Line one"));
        assert!(combined.contains("Line two"));
    }

    #[test]
    fn test_full_document() {
        let html = r#"<!DOCTYPE html>
        <html>
        <head><title>Test</title></head>
        <body>
            <h1>Document Title</h1>
            <p>Introduction paragraph with <strong>bold</strong> text.</p>
            <h2>Section</h2>
            <ul>
                <li>Item 1</li>
                <li>Item 2</li>
            </ul>
            <pre><code class="language-python">print("hello")</code></pre>
            <hr>
            <p>Conclusion</p>
        </body>
        </html>"#;
        let elements = parse_html(html);
        assert!(elements.len() >= 7);
        assert!(
            elements.iter().any(
                |e| matches!(e, Element::Heading { level: 1, text } if text == "Document Title")
            )
        );
        assert!(
            elements
                .iter()
                .any(|e| matches!(e, Element::CodeBlock { language, .. } if language == "python"))
        );
        assert!(
            elements
                .iter()
                .any(|e| matches!(e, Element::HorizontalRule))
        );
    }

    #[test]
    fn test_nested_lists() {
        let html = "<ul><li>Outer</li><li>Inner<ul><li>Deep</li></ul></li></ul>";
        let elements = parse_html(html);
        // The inner <ul> inside <li> will produce items from recursion in collect_text
        // but the current implementation flattens — at minimum we get the outer items.
        assert!(!elements.is_empty());
    }

    #[test]
    fn test_empty_input() {
        let elements = parse_html("");
        assert!(elements.is_empty());
    }

    #[test]
    fn test_whitespace_only() {
        let elements = parse_html("   \n\t  ");
        assert!(elements.is_empty());
    }

    #[test]
    fn test_html_to_pdf_bytes() {
        let html = "<h1>Test</h1><p>Hello world</p>";
        let bytes = html_to_pdf_bytes(html, "Helvetica", 12.0).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_inline_style_bold() {
        let elements = parse_html(r#"<p>Hello <span style="font-weight: bold">world</span></p>"#);
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::Bold(t) if t == "world"))
                );
            }
            _ => panic!("expected RichParagraph"),
        }
    }

    #[test]
    fn test_inline_style_italic() {
        let elements = parse_html(r#"<p>Hello <span style="font-style: italic">world</span></p>"#);
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::Italic(t) if t == "world"))
                );
            }
            _ => panic!("expected RichParagraph"),
        }
    }

    #[test]
    fn test_inline_style_bold_italic() {
        let elements =
            parse_html(r#"<p><span style="font-weight: bold; font-style: italic">both</span></p>"#);
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::BoldItalic(t) if t == "both"))
                );
            }
            _ => panic!("expected RichParagraph"),
        }
    }

    #[test]
    fn test_style_tag_bold() {
        let html = r#"<style>p { font-weight: bold; }</style><p>Bold text</p>"#;
        let elements = parse_html(html);
        // The <style> tag should not produce any elements.
        // The <p> should produce a RichParagraph with bold text.
        assert!(!elements.is_empty());
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::Bold(t) if t.contains("Bold text")))
                );
            }
            _ => panic!("expected RichParagraph with bold text"),
        }
    }

    #[test]
    fn test_style_tag_italic() {
        let html = r#"<style>p { font-style: italic; }</style><p>Italic text</p>"#;
        let elements = parse_html(html);
        assert!(!elements.is_empty());
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::Italic(t) if t.contains("Italic text")))
                );
            }
            _ => panic!("expected RichParagraph with italic text"),
        }
    }

    #[test]
    fn test_style_tag_class_selector() {
        let html = r#"<style>.highlight { font-weight: bold; }</style><p class="highlight">Bold via class</p>"#;
        let elements = parse_html(html);
        assert!(!elements.is_empty());
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::Bold(t) if t.contains("Bold via class")))
                );
            }
            _ => panic!("expected RichParagraph with bold text from class selector"),
        }
    }

    #[test]
    fn test_style_tag_does_not_produce_elements() {
        let html = r#"<style>body { color: red; }</style>"#;
        let elements = parse_html(html);
        assert!(elements.is_empty(), "style tag should not produce elements");
    }

    #[test]
    fn test_css_text_align_table() {
        let html = r#"<style>th.right { text-align: right; }</style>
        <table><thead><tr><th>Name</th><th class="right">Price</th></tr></thead>
        <tbody><tr><td>Item</td><td>$10</td></tr></tbody></table>"#;
        let elements = parse_html(html);
        // Should have a separator row with Right alignment on second column.
        assert!(elements.iter().any(|e| matches!(
            e,
            Element::TableRow { is_separator: true, alignments, .. }
            if alignments.len() >= 2 && alignments[1] == TableAlignment::Right
        )));
    }

    #[test]
    fn test_inline_style_text_align_table() {
        let html = r#"<table><thead><tr><th>Name</th><th style="text-align: center">Price</th></tr></thead>
        <tbody><tr><td>Item</td><td>$10</td></tr></tbody></table>"#;
        let elements = parse_html(html);
        assert!(elements.iter().any(|e| matches!(
            e,
            Element::TableRow { is_separator: true, alignments, .. }
            if alignments.len() >= 2 && alignments[1] == TableAlignment::Center
        )));
    }

    #[test]
    fn test_css_overrides_inline() {
        let html =
            r#"<style>p { font-weight: bold; }</style><p style="font-style: italic">Both</p>"#;
        let elements = parse_html(html);
        assert!(!elements.is_empty());
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::BoldItalic(t) if t == "Both"))
                );
            }
            _ => panic!("expected RichParagraph with bold+italic"),
        }
    }

    #[test]
    fn test_css_tag_class_selector() {
        let html =
            r#"<style>p.special { font-style: italic; }</style><p class="special">Special</p>"#;
        let elements = parse_html(html);
        assert!(!elements.is_empty());
        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::Italic(t) if t.contains("Special")))
                );
            }
            _ => panic!("expected RichParagraph with italic from tag.class selector"),
        }
    }

    #[test]
    fn deeply_nested_html_does_not_recurse_forever() {
        // 5000 nested <div>s must terminate (depth-capped recursion).
        let depth = 5000;
        let html = format!(
            "<html><body>{}<p>core text</p>{}</body></html>",
            "<div>".repeat(depth),
            "</div>".repeat(depth)
        );
        let elements = super::parse_html(&html);
        assert!(
            elements
                .iter()
                .any(|e| matches!(e, Element::Paragraph { text } if text.contains("core"))),
            "text at the bottom of a deep tree must still convert"
        );
    }
}
