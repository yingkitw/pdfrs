//! HTML-to-PDF conversion via the existing Element pipeline.
//!
//! This module provides a lightweight HTML parser that converts common HTML
//! documents into [`Element`] vectors, which are then rendered to PDF using
//! the existing [`crate::pdf_generator`] pipeline.
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

use crate::elements::{Element, TableAlignment, TextSegment};

/// A parsed HTML node in a simplified DOM tree.
#[derive(Debug, Clone)]
enum Node {
    /// Plain text content.
    Text(String),
    /// An element with tag name, attributes, and children.
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<Node>,
    },
    /// A self-closing / void element (e.g. `<br>`, `<hr>`, `<img>`).
    Void {
        tag: String,
        attrs: Vec<(String, String)>,
    },
}

/// HTML void elements that never have closing tags.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// Parse an HTML string into a vector of [`Element`]s.
pub fn parse_html(html: &str) -> Vec<Element> {
    let nodes = tokenize(html);
    let mut elements = Vec::new();
    for node in &nodes {
        convert_node(node, &mut elements, 0, 1);
    }
    elements
}

/// Convert HTML to PDF file.
pub fn html_to_pdf(
    filename: &str,
    html: &str,
    font: &str,
    base_font_size: f32,
) -> anyhow::Result<()> {
    let elements = parse_html(html);
    let layout = crate::pdf_generator::PageLayout::portrait();
    crate::pdf_generator::create_pdf_from_elements_with_layout(
        filename,
        &elements,
        font,
        base_font_size,
        layout,
    )
}

/// Convert HTML to PDF bytes in memory.
pub fn html_to_pdf_bytes(html: &str, font: &str, base_font_size: f32) -> anyhow::Result<Vec<u8>> {
    let elements = parse_html(html);
    let layout = crate::pdf_generator::PageLayout::portrait();
    crate::pdf_generator::generate_pdf_bytes_internal(
        &elements,
        font,
        base_font_size,
        layout,
        None,
        true,
        None,
    )
}

// ---------------------------------------------------------------------------
// Tokenizer / Parser
// ---------------------------------------------------------------------------

/// Tokenize an HTML string into a list of top-level nodes.
fn tokenize(html: &str) -> Vec<Node> {
    let chars: Vec<char> = html.chars().collect();
    let mut pos = 0;
    let mut stack: Vec<Node> = Vec::new();
    let mut root_children: Vec<Node> = Vec::new();

    while pos < chars.len() {
        if chars[pos] == '<' {
            // Could be a tag, comment, or doctype
            if chars[pos..].starts_with(&['<', '!', '-', '-']) {
                // Skip comment
                if let Some(end) = find_substring(&chars, pos, "-->") {
                    pos = end + 3;
                } else {
                    pos = chars.len();
                }
            } else if chars[pos..].starts_with(&['<', '!']) {
                // Skip doctype/declaration
                if let Some(end) = find_char(&chars, pos, '>') {
                    pos = end + 1;
                } else {
                    pos = chars.len();
                }
            } else if chars[pos..].starts_with(&['<', '/']) {
                // Closing tag
                if let Some(end) = find_char(&chars, pos, '>') {
                    let tag: String = chars[pos + 2..end].iter().collect();
                    let tag = tag.trim().to_lowercase();
                    pos = end + 1;

                    // Pop stack until we find matching open tag
                    while let Some(top) = stack.pop() {
                        if let Node::Element { tag: t, .. } = &top
                            && t == &tag
                        {
                            if let Some(parent) = stack.last_mut() {
                                if let Node::Element { children, .. } = parent {
                                    children.push(top);
                                }
                            } else {
                                root_children.push(top);
                            }
                            break;
                        }
                        // Unmatched — push children to parent
                        if let Some(parent) = stack.last_mut() {
                            if let Node::Element { children, .. } = parent {
                                if let Node::Element {
                                    children: child_children,
                                    ..
                                } = &top
                                {
                                    children.extend(child_children.clone());
                                } else {
                                    children.push(top);
                                }
                            }
                        } else {
                            root_children.push(top);
                        }
                    }
                } else {
                    pos = chars.len();
                }
            } else {
                // Opening tag
                if let Some(end) = find_char(&chars, pos, '>') {
                    let raw: String = chars[pos + 1..end].iter().collect();
                    pos = end + 1;

                    let (tag, attrs, self_closing) = parse_tag(&raw);
                    let tag = tag.to_lowercase();

                    if self_closing || VOID_ELEMENTS.contains(&tag.as_str()) {
                        let node = Node::Void { tag, attrs };
                        if let Some(Node::Element { children, .. }) = stack.last_mut() {
                            children.push(node);
                        } else {
                            root_children.push(node);
                        }
                    } else {
                        stack.push(Node::Element {
                            tag,
                            attrs,
                            children: Vec::new(),
                        });
                    }
                } else {
                    pos = chars.len();
                }
            }
        } else {
            // Text content
            let start = pos;
            while pos < chars.len() && chars[pos] != '<' {
                pos += 1;
            }
            let text: String = chars[start..pos].iter().collect();
            let decoded = decode_entities(&text);
            if !decoded.is_empty() {
                let node = Node::Text(decoded);
                if let Some(Node::Element { children, .. }) = stack.last_mut() {
                    children.push(node);
                } else {
                    root_children.push(node);
                }
            }
        }
    }

    // Close any unclosed tags
    while let Some(top) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            if let Node::Element { children, .. } = parent {
                children.push(top);
            }
        } else {
            root_children.push(top);
        }
    }

    root_children
}

/// Find the first occurrence of a substring starting from `pos`.
fn find_substring(chars: &[char], pos: usize, needle: &str) -> Option<usize> {
    let needle_chars: Vec<char> = needle.chars().collect();
    let n = needle_chars.len();
    if pos + n > chars.len() {
        return None;
    }
    for i in pos..=chars.len().saturating_sub(n) {
        if chars[i..i + n] == needle_chars[..] {
            return Some(i);
        }
    }
    None
}

/// Find the first occurrence of a character starting from `pos`.
fn find_char(chars: &[char], pos: usize, ch: char) -> Option<usize> {
    chars[pos..].iter().position(|&c| c == ch).map(|p| pos + p)
}

/// Parse a tag's inner content (between `<` and `>`).
/// Returns (tag_name, attributes, self_closing).
fn parse_tag(raw: &str) -> (String, Vec<(String, String)>, bool) {
    let raw = raw.trim();
    let self_closing = raw.ends_with('/');
    let raw = raw.trim_end_matches('/').trim();

    let mut parts = raw.splitn(2, char::is_whitespace);
    let tag = parts.next().unwrap_or("").to_string();
    let rest = parts.next().unwrap_or("");

    let attrs = parse_attributes(rest);
    (tag, attrs, self_closing)
}

/// Parse HTML attributes from a string like `key="value" key2='value2' key3`.
fn parse_attributes(s: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Skip whitespace
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }

        // Read attribute name
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            i += 1;
        }
        let name: String = chars[start..i].iter().collect();
        if name.is_empty() {
            i += 1;
            continue;
        }

        // Skip whitespace
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }

        // Check for value
        let value = if i < chars.len() && chars[i] == '=' {
            i += 1; // skip =
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                let quote = chars[i];
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != quote {
                    i += 1;
                }
                let val: String = chars[start..i].iter().collect();
                if i < chars.len() {
                    i += 1; // skip closing quote
                }
                val
            } else {
                let start = i;
                while i < chars.len() && !chars[i].is_whitespace() {
                    i += 1;
                }
                chars[start..i].iter().collect()
            }
        } else {
            String::new()
        };

        attrs.push((name.to_lowercase(), decode_entities(&value)));
    }

    attrs
}

/// Decode common HTML entities.
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", "\u{00A0}")
        .replace("&copy;", "\u{00A9}")
        .replace("&reg;", "\u{00AE}")
        .replace("&trade;", "\u{2122}")
        .replace("&hellip;", "...")
        .replace("&mdash;", "\u{2014}")
        .replace("&ndash;", "\u{2013}")
        .replace("&ldquo;", "\u{201C}")
        .replace("&rdquo;", "\u{201D}")
        .replace("&lsquo;", "\u{2018}")
        .replace("&rsquo;", "\u{2019}")
}

/// Get an attribute value from a list.
fn get_attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

// ---------------------------------------------------------------------------
// Node → Element conversion
// ---------------------------------------------------------------------------

/// Convert a parsed HTML node into Element(s), appending to `out`.
fn convert_node(node: &Node, out: &mut Vec<Element>, list_depth: u8, _ol_counter: u32) {
    match node {
        Node::Text(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                out.push(Element::Paragraph {
                    text: trimmed.to_string(),
                });
            }
        }
        Node::Void { tag, attrs } => {
            match tag.as_str() {
                "hr" => out.push(Element::HorizontalRule),
                "br" => {
                    // Inline br handling: append a newline to the last paragraph.
                    if let Some(Element::Paragraph { text }) = out.last_mut() {
                        text.push('\n');
                    }
                }
                "img" => {
                    let alt = get_attr(attrs, "alt").unwrap_or("").to_string();
                    let src = get_attr(attrs, "src").unwrap_or("").to_string();
                    if !src.is_empty() {
                        out.push(Element::Image { alt, path: src });
                    }
                }
                _ => {}
            }
        }
        Node::Element {
            tag,
            attrs,
            children,
        } => {
            match tag.as_str() {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    let level = tag[1..].parse::<u8>().unwrap_or(1);
                    let text = collect_text(children);
                    out.push(Element::Heading {
                        level,
                        text: text.trim().to_string(),
                    });
                }
                "p" => {
                    let segments = collect_rich_segments(children);
                    if segments.len() == 1
                        && let TextSegment::Plain(text) = &segments[0]
                    {
                        out.push(Element::Paragraph {
                            text: text.trim().to_string(),
                        });
                        return;
                    }
                    if !segments.is_empty() {
                        out.push(Element::RichParagraph { segments });
                    }
                }
                "strong" | "b" => {
                    // Inline — should be handled by parent's collect_rich_segments.
                    // If we reach here as a standalone, emit as paragraph.
                    let segments = collect_rich_segments(children);
                    if !segments.is_empty() {
                        out.push(Element::RichParagraph { segments });
                    }
                }
                "em" | "i" => {
                    let segments = collect_rich_segments(children);
                    if !segments.is_empty() {
                        out.push(Element::RichParagraph { segments });
                    }
                }
                "pre" => {
                    // Look for <code> child for language hint.
                    let (language, code) = if let Some(Node::Element {
                        tag: child_tag,
                        attrs: child_attrs,
                        children,
                    }) = children.first()
                    {
                        if child_tag == "code" {
                            let lang = get_attr(child_attrs, "class")
                                .and_then(|c| {
                                    c.split_whitespace().find(|s| s.starts_with("language-"))
                                })
                                .map(|s| s.trim_start_matches("language-").to_string())
                                .unwrap_or_default();
                            (lang, collect_text(children))
                        } else {
                            (String::new(), collect_text(children))
                        }
                    } else {
                        (String::new(), collect_text(children))
                    };
                    out.push(Element::CodeBlock {
                        language,
                        code: code.trim_end().to_string(),
                    });
                }
                "code" => {
                    // Inline code (not inside <pre>)
                    let text = collect_text(children);
                    if !text.is_empty() {
                        out.push(Element::InlineCode { code: text });
                    }
                }
                "ul" => {
                    for child in children {
                        if let Node::Element {
                            tag: ctag,
                            children: li_children,
                            ..
                        } = child
                            && ctag == "li"
                        {
                            let text = collect_text(li_children);
                            out.push(Element::UnorderedListItem {
                                text: text.trim().to_string(),
                                depth: list_depth,
                            });
                        }
                    }
                }
                "ol" => {
                    let mut num = 1u32;
                    for child in children {
                        if let Node::Element {
                            tag: ctag,
                            children: li_children,
                            ..
                        } = child
                            && ctag == "li"
                        {
                            let text = collect_text(li_children);
                            out.push(Element::OrderedListItem {
                                number: num,
                                text: text.trim().to_string(),
                                depth: list_depth,
                            });
                            num += 1;
                        }
                    }
                }
                "li" => {
                    // Standalone <li> (outside ul/ol) — treat as unordered item.
                    let text = collect_text(children);
                    out.push(Element::UnorderedListItem {
                        text: text.trim().to_string(),
                        depth: list_depth,
                    });
                }
                "blockquote" => {
                    let text = collect_text(children);
                    out.push(Element::BlockQuote {
                        text: text.trim().to_string(),
                        depth: 0,
                    });
                }
                "table" => {
                    convert_table(children, out);
                }
                "a" => {
                    let href = get_attr(attrs, "href").unwrap_or("").to_string();
                    let text = collect_text(children);
                    if !href.is_empty() {
                        out.push(Element::Link {
                            text: text.trim().to_string(),
                            url: href,
                        });
                    } else {
                        out.push(Element::Paragraph {
                            text: text.trim().to_string(),
                        });
                    }
                }
                "img" => {
                    let alt = get_attr(attrs, "alt").unwrap_or("").to_string();
                    let src = get_attr(attrs, "src").unwrap_or("").to_string();
                    if !src.is_empty() {
                        out.push(Element::Image { alt, path: src });
                    }
                }
                "br" => {
                    if let Some(Element::Paragraph { text }) = out.last_mut() {
                        text.push('\n');
                    }
                }
                "hr" => out.push(Element::HorizontalRule),
                "div" | "section" | "article" | "main" | "header" | "footer" | "nav" | "aside" => {
                    // Container elements — recurse into children.
                    for child in children {
                        convert_node(child, out, list_depth, _ol_counter);
                    }
                }
                "span" => {
                    // Inline span — treat like a paragraph with rich segments.
                    let segments = collect_rich_segments(children);
                    if !segments.is_empty() {
                        out.push(Element::RichParagraph { segments });
                    }
                }
                _ => {
                    // Unknown element — recurse into children.
                    for child in children {
                        convert_node(child, out, list_depth, _ol_counter);
                    }
                }
            }
        }
    }
}

/// Convert a `<table>` node's children into TableRow elements.
fn convert_table(table_children: &[Node], out: &mut Vec<Element>) {
    let mut has_header = false;
    let mut alignments: Vec<TableAlignment> = Vec::new();

    // Collect all <tr> rows from <thead>, <tbody>, or direct children.
    let mut rows: Vec<&[Node]> = Vec::new();

    for child in table_children {
        if let Node::Element { tag, children, .. } = child {
            match tag.as_str() {
                "thead" | "tbody" | "tfoot" => {
                    for row in children {
                        if let Node::Element {
                            tag: rt,
                            children: rc,
                            ..
                        } = row
                            && rt == "tr"
                        {
                            rows.push(rc);
                        }
                    }
                }
                "tr" => rows.push(children),
                _ => {}
            }
        }
    }

    for (row_idx, row_children) in rows.iter().enumerate() {
        let mut cells = Vec::new();
        let mut is_header = false;

        for cell in row_children.iter() {
            if let Node::Element {
                tag,
                children,
                attrs,
                ..
            } = cell
                && (tag == "th" || tag == "td")
            {
                if tag == "th" {
                    is_header = true;
                }
                let text = collect_text(children);
                cells.push(text.trim().to_string());

                // Collect alignment from style attribute on first row.
                if row_idx == 0
                    && let Some(style) = get_attr(attrs, "style")
                {
                    let align = if style.contains("text-align: center")
                        || style.contains("text-align:center")
                    {
                        Some(TableAlignment::Center)
                    } else if style.contains("text-align: right")
                        || style.contains("text-align:right")
                    {
                        Some(TableAlignment::Right)
                    } else {
                        None
                    };
                    if let Some(a) = align {
                        while alignments.len() < cells.len() {
                            alignments.push(TableAlignment::Left);
                        }
                        alignments[cells.len() - 1] = a;
                    }
                }
            }
        }

        if cells.is_empty() {
            continue;
        }

        if row_idx == 0 && is_header {
            has_header = true;
        }

        // Emit separator row after header.
        if has_header && row_idx == 1 {
            let sep_alignments: Vec<TableAlignment> = if !alignments.is_empty() {
                alignments.clone()
            } else {
                vec![TableAlignment::Left; cells.len()]
            };
            out.push(Element::TableRow {
                cells: cells.iter().map(|_| "---".to_string()).collect(),
                is_separator: true,
                alignments: sep_alignments,
            });
        }

        out.push(Element::TableRow {
            cells,
            is_separator: false,
            alignments: if row_idx == 0 && !alignments.is_empty() {
                alignments.clone()
            } else {
                Vec::new()
            },
        });
    }
}

/// Collect all text content from a node's children, recursively.
fn collect_text(nodes: &[Node]) -> String {
    let mut s = String::new();
    for node in nodes {
        collect_text_inner(node, &mut s);
    }
    s
}

fn collect_text_inner(node: &Node, out: &mut String) {
    match node {
        Node::Text(text) => out.push_str(text),
        Node::Void { tag, .. } => {
            if tag == "br" {
                out.push('\n');
            }
        }
        Node::Element { children, .. } => {
            for child in children {
                collect_text_inner(child, out);
            }
        }
    }
}

/// Collect rich text segments from children, handling inline formatting.
fn collect_rich_segments(nodes: &[Node]) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    for node in nodes {
        collect_segments_inner(node, &mut segments, false, false);
    }
    if segments.is_empty() {
        return segments;
    }
    // Merge adjacent plain segments.
    let mut merged: Vec<TextSegment> = Vec::new();
    for seg in segments {
        match (&seg, merged.last_mut()) {
            (TextSegment::Plain(new), Some(TextSegment::Plain(prev))) => {
                prev.push_str(new);
            }
            _ => merged.push(seg),
        }
    }
    // Trim leading/trailing whitespace from first/last plain segment.
    if let Some(TextSegment::Plain(s)) = merged.first_mut() {
        *s = s.trim_start().to_string();
    }
    if let Some(TextSegment::Plain(s)) = merged.last_mut() {
        *s = s.trim_end().to_string();
    }
    // Remove empty plain segments.
    merged.retain(|s| !matches!(s, TextSegment::Plain(p) if p.is_empty()));
    merged
}

fn collect_segments_inner(node: &Node, out: &mut Vec<TextSegment>, bold: bool, italic: bool) {
    match node {
        Node::Text(text) => {
            let text = text.clone();
            if bold && italic {
                out.push(TextSegment::BoldItalic(text));
            } else if bold {
                out.push(TextSegment::Bold(text));
            } else if italic {
                out.push(TextSegment::Italic(text));
            } else {
                out.push(TextSegment::Plain(text));
            }
        }
        Node::Void { tag, .. } => {
            if tag == "br" {
                out.push(TextSegment::Plain("\n".to_string()));
            }
        }
        Node::Element {
            tag,
            attrs,
            children,
        } => {
            match tag.as_str() {
                "strong" | "b" => {
                    for child in children {
                        collect_segments_inner(child, out, true, italic);
                    }
                }
                "em" | "i" => {
                    for child in children {
                        collect_segments_inner(child, out, bold, true);
                    }
                }
                "code" => {
                    let text = collect_text(children);
                    out.push(TextSegment::Code(text));
                }
                "a" => {
                    let href = get_attr(attrs, "href").unwrap_or("").to_string();
                    let text = collect_text(children);
                    if !href.is_empty() {
                        out.push(TextSegment::Link { text, url: href });
                    } else {
                        out.push(TextSegment::Plain(text));
                    }
                }
                "s" | "del" | "strike" => {
                    let text = collect_text(children);
                    out.push(TextSegment::Strikethrough(text));
                }
                "br" => {
                    out.push(TextSegment::Plain("\n".to_string()));
                }
                "span" => {
                    // Check for style-based bold/italic.
                    let style_bold = get_attr(attrs, "style")
                        .map(|s| s.contains("font-weight: bold") || s.contains("font-weight:bold"))
                        .unwrap_or(false);
                    let style_italic = get_attr(attrs, "style")
                        .map(|s| {
                            s.contains("font-style: italic") || s.contains("font-style:italic")
                        })
                        .unwrap_or(false);
                    let new_bold = bold || style_bold;
                    let new_italic = italic || style_italic;
                    for child in children {
                        collect_segments_inner(child, out, new_bold, new_italic);
                    }
                }
                _ => {
                    // Unknown inline element — recurse with current formatting state.
                    for child in children {
                        collect_segments_inner(child, out, bold, italic);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
}
