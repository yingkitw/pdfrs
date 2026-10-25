//! DOM-to-`Element` conversion and the public html-to-PDF entry points.

use super::css::{CssRule, compute_style, parse_stylesheet};
use super::tokenizer::{Node, get_attr, tokenize};
use crate::elements::{Element, TableAlignment, TextSegment};

pub fn parse_html(html: &str) -> Vec<Element> {
    let nodes = tokenize(html);
    // Extract CSS rules from <style> tags.
    let css_rules = extract_css_rules(&nodes);
    let mut elements = Vec::new();
    for node in &nodes {
        convert_node(node, &mut elements, 0, 1, &css_rules, 0);
    }
    elements
}

/// Recursively find all `<style>` nodes and parse their text content as CSS.
fn extract_css_rules(nodes: &[Node]) -> Vec<CssRule> {
    let mut rules = Vec::new();
    for node in nodes {
        extract_css_rules_inner(node, &mut rules);
    }
    rules
}

fn extract_css_rules_inner(node: &Node, rules: &mut Vec<CssRule>) {
    match node {
        Node::Element { tag, children, .. } if tag == "style" => {
            let css = collect_text(children);
            rules.extend(parse_stylesheet(&css));
        }
        Node::Element { children, .. } => {
            for child in children {
                extract_css_rules_inner(child, rules);
            }
        }
        _ => {}
    }
}

/// Convert HTML to PDF file.
pub fn html_to_pdf(
    filename: &str,
    html: &str,
    font: &str,
    base_font_size: f32,
) -> crate::error::Result<()> {
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
pub fn html_to_pdf_bytes(
    html: &str,
    font: &str,
    base_font_size: f32,
) -> crate::error::Result<Vec<u8>> {
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

// ---------------------------------------------------------------------------
// Node → Element conversion
// ---------------------------------------------------------------------------

/// Maximum DOM depth the converter descends recursively; deeper subtrees are
/// flattened iteratively (guards stack usage without losing content).
const MAX_DOM_DEPTH: u8 = 128;

/// Iteratively collect text from a subtree (no recursion — stack-safe at any depth).
fn collect_text_flat(nodes: &[Node]) -> String {
    let mut out = String::new();
    let mut stack: Vec<&Node> = nodes.iter().rev().collect();
    while let Some(node) = stack.pop() {
        match node {
            Node::Text(text) => out.push_str(text),
            Node::Void { tag, .. } => {
                if tag == "br" {
                    out.push('\n');
                }
            }
            Node::Element { children, .. } => {
                stack.extend(children.iter().rev());
            }
        }
    }
    out
}

/// Convert a parsed HTML node into Element(s), appending to `out`.
fn convert_node(
    node: &Node,
    out: &mut Vec<Element>,
    list_depth: u8,
    _ol_counter: u32,
    css_rules: &[CssRule],
    depth: u8,
) {
    if depth > MAX_DOM_DEPTH {
        // Too deep for recursion: salvage the subtree's text iteratively.
        if let Node::Element { children, .. } = node {
            let text = collect_text_flat(children);
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                out.push(Element::Paragraph {
                    text: trimmed.to_string(),
                });
            }
        }
        return;
    }
    match node {
        Node::Text(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                out.push(Element::Paragraph {
                    text: trimmed.to_string(),
                });
            }
        }
        Node::Void { tag, attrs } => match tag.as_str() {
            "hr" => out.push(Element::HorizontalRule),
            "br" => {
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
        },
        Node::Element {
            tag,
            attrs,
            children,
        } => {
            let style = compute_style(tag, attrs, css_rules);
            match tag.as_str() {
                "style" => {
                    // CSS rules already extracted; skip output.
                }
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    let level = tag[1..].parse::<u8>().unwrap_or(1);
                    let text = collect_text(children);
                    out.push(Element::Heading {
                        level,
                        text: text.trim().to_string(),
                    });
                }
                "p" => {
                    let segments =
                        collect_rich_segments_styled(children, style.bold, style.italic, css_rules);
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
                    let segments =
                        collect_rich_segments_styled(children, true, style.italic, css_rules);
                    if !segments.is_empty() {
                        out.push(Element::RichParagraph { segments });
                    }
                }
                "em" | "i" => {
                    let segments =
                        collect_rich_segments_styled(children, style.bold, true, css_rules);
                    if !segments.is_empty() {
                        out.push(Element::RichParagraph { segments });
                    }
                }
                "pre" => {
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
                    convert_table(children, out, css_rules);
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
                    for child in children {
                        convert_node(child, out, list_depth, _ol_counter, css_rules, depth + 1);
                    }
                }
                "span" => {
                    let segments =
                        collect_rich_segments_styled(children, style.bold, style.italic, css_rules);
                    if !segments.is_empty() {
                        out.push(Element::RichParagraph { segments });
                    }
                }
                _ => {
                    for child in children {
                        convert_node(child, out, list_depth, _ol_counter, css_rules, depth + 1);
                    }
                }
            }
        }
    }
}

/// Convert a `<table>` node's children into TableRow elements.
fn convert_table(table_children: &[Node], out: &mut Vec<Element>, css_rules: &[CssRule]) {
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
        let mut colspans = Vec::new();
        let mut rowspans = Vec::new();
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

                let colspan = attrs
                    .iter()
                    .find(|(k, _)| k == "colspan")
                    .and_then(|(_, v)| v.parse::<u32>().ok())
                    .unwrap_or(1);
                let rowspan = attrs
                    .iter()
                    .find(|(k, _)| k == "rowspan")
                    .and_then(|(_, v)| v.parse::<u32>().ok())
                    .unwrap_or(1);
                colspans.push(colspan);
                rowspans.push(rowspan);

                // Collect alignment from CSS or style attribute on first row.
                if row_idx == 0 {
                    let style = compute_style(tag, attrs, css_rules);
                    let align = if let Some(ta) = &style.text_align {
                        match ta.as_str() {
                            "center" => Some(TableAlignment::Center),
                            "right" => Some(TableAlignment::Right),
                            "justify" => Some(TableAlignment::Justify),
                            _ => None,
                        }
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
                colspans: Vec::new(),
                rowspans: Vec::new(),
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
            colspans,
            rowspans,
        });
    }
}

/// Collect all text content from a node's children, recursively.
fn collect_text(nodes: &[Node]) -> String {
    let mut s = String::new();
    for node in nodes {
        collect_text_inner(node, &mut s, 0);
    }
    s
}

fn collect_text_inner(node: &Node, out: &mut String, depth: u8) {
    if depth > MAX_DOM_DEPTH {
        out.push_str(&collect_text_flat(std::slice::from_ref(node)));
        return;
    }
    match node {
        Node::Text(text) => out.push_str(text),
        Node::Void { tag, .. } => {
            if tag == "br" {
                out.push('\n');
            }
        }
        Node::Element { children, .. } => {
            for child in children {
                collect_text_inner(child, out, depth + 1);
            }
        }
    }
}

/// Collect rich text segments from children, with inherited bold/italic from CSS.
fn collect_rich_segments_styled(
    nodes: &[Node],
    bold: bool,
    italic: bool,
    css_rules: &[CssRule],
) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    for node in nodes {
        collect_segments_styled_inner(node, &mut segments, bold, italic, css_rules, 0);
    }
    if segments.is_empty() {
        return segments;
    }
    let mut merged: Vec<TextSegment> = Vec::new();
    for seg in segments {
        match (&seg, merged.last_mut()) {
            (TextSegment::Plain(new), Some(TextSegment::Plain(prev))) => {
                prev.push_str(new);
            }
            _ => merged.push(seg),
        }
    }
    if let Some(TextSegment::Plain(s)) = merged.first_mut() {
        *s = s.trim_start().to_string();
    }
    if let Some(TextSegment::Plain(s)) = merged.last_mut() {
        *s = s.trim_end().to_string();
    }
    merged.retain(|s| !matches!(s, TextSegment::Plain(p) if p.is_empty()));
    merged
}

fn collect_segments_styled_inner(
    node: &Node,
    out: &mut Vec<TextSegment>,
    bold: bool,
    italic: bool,
    css_rules: &[CssRule],
    depth: u8,
) {
    if depth > MAX_DOM_DEPTH {
        return;
    }
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
            let style = compute_style(tag, attrs, css_rules);
            let new_bold = bold || style.bold;
            let new_italic = italic || style.italic;
            match tag.as_str() {
                "strong" | "b" => {
                    for child in children {
                        collect_segments_styled_inner(
                            child,
                            out,
                            true,
                            new_italic,
                            css_rules,
                            depth + 1,
                        );
                    }
                }
                "em" | "i" => {
                    for child in children {
                        collect_segments_styled_inner(
                            child,
                            out,
                            new_bold,
                            true,
                            css_rules,
                            depth + 1,
                        );
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
                    for child in children {
                        collect_segments_styled_inner(
                            child,
                            out,
                            new_bold,
                            new_italic,
                            css_rules,
                            depth + 1,
                        );
                    }
                }
                _ => {
                    for child in children {
                        collect_segments_styled_inner(
                            child,
                            out,
                            new_bold,
                            new_italic,
                            css_rules,
                            depth + 1,
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
