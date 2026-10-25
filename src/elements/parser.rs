//! Block-level Markdown parser (`parse_markdown` / `parse_markdown_with_hook`).
use super::inline::{has_inline_formatting, parse_inline_formatting, strip_inline_formatting};
use super::types::parse_cell_alignment;
use super::{Element, PageNumberStyle, TableAlignment, re_footnote_ref, re_img, re_link_line};

pub fn parse_markdown(markdown: &str) -> Vec<Element> {
    parse_markdown_with_hook(markdown, None)
}

/// Parse Markdown with an optional line-level hook (used by the plugin system).
///
/// The hook runs for each line outside code/math fences. Returning
/// `Some((elements, consumed))` appends those elements and skips `consumed` lines.
#[allow(clippy::type_complexity)]
pub fn parse_markdown_with_hook(
    markdown: &str,
    hook: Option<&dyn Fn(&[&str], usize) -> Option<(Vec<Element>, usize)>>,
) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_buf = String::new();
    let mut in_math_block = false;
    let mut math_buf = String::new();
    let lines: Vec<&str> = markdown.lines().collect();
    let img_re = re_img();
    let link_re = re_link_line();
    let footnote_ref_re = re_footnote_ref();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Math block toggle ($$...$$)
        if trimmed.starts_with("$$") && !in_code_block && !in_math_block {
            // Check if it's a single-line math block: $$ expr $$
            let rest = trimmed[2..].trim();
            if rest.ends_with("$$") && rest.len() > 2 {
                let expr = rest[..rest.len() - 2].trim().to_string();
                elements.push(Element::MathBlock { expression: expr });
                i += 1;
                continue;
            }
            in_math_block = true;
            math_buf.clear();
            i += 1;
            continue;
        }

        if in_math_block {
            if trimmed == "$$" {
                elements.push(Element::MathBlock {
                    expression: math_buf.clone(),
                });
                math_buf.clear();
                in_math_block = false;
            } else {
                if !math_buf.is_empty() {
                    math_buf.push('\n');
                }
                math_buf.push_str(trimmed);
            }
            i += 1;
            continue;
        }

        // Code block toggle (chart fences become Element::Chart)
        if trimmed.starts_with("```") {
            if in_code_block {
                if let Some(chart) = crate::chart::try_parse_chart_element(&code_lang, &code_buf) {
                    elements.push(chart);
                } else {
                    elements.push(Element::CodeBlock {
                        language: code_lang.clone(),
                        code: code_buf.clone(),
                    });
                }
                code_buf.clear();
                code_lang.clear();
                in_code_block = false;
            } else {
                in_code_block = true;
                code_lang = trimmed
                    .strip_prefix("```")
                    .unwrap_or(trimmed)
                    .trim()
                    .to_string();
            }
            i += 1;
            continue;
        }

        if in_code_block {
            if !code_buf.is_empty() {
                code_buf.push('\n');
            }
            code_buf.push_str(line);
            i += 1;
            continue;
        }

        // Plugin hook (before built-in block syntax)
        if let Some(h) = hook
            && let Some((plugin_elements, consumed)) = h(&lines, i)
        {
            elements.extend(plugin_elements);
            i += consumed.max(1);
            continue;
        }

        // Empty line
        if trimmed.is_empty() {
            elements.push(Element::EmptyLine);
            i += 1;
            continue;
        }

        // Horizontal rule
        if (trimmed == "---" || trimmed == "***" || trimmed == "___") && trimmed.len() >= 3 {
            elements.push(Element::HorizontalRule);
            i += 1;
            continue;
        }

        // Headings
        if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|&c| c == '#').count().min(6) as u8;
            let text = trimmed[level as usize..].trim().to_string();
            elements.push(Element::Heading { level, text });
            i += 1;
            continue;
        }

        // Page break: <!-- pagebreak --> or \pagebreak
        if trimmed == "<!-- pagebreak -->" || trimmed == "\\pagebreak" {
            elements.push(Element::PageBreak);
            i += 1;
            continue;
        }

        // Multi-column / thesis switches: <!-- columns:2 -->, <!-- toc -->, etc.
        if let Some(rest) = trimmed
            .strip_prefix("<!--")
            .and_then(|s| s.strip_suffix("-->"))
        {
            let inner = rest.trim().to_ascii_lowercase();
            if let Some(n) = inner
                .strip_prefix("columns:")
                .or_else(|| inner.strip_prefix("columns :"))
                .map(str::trim)
                .and_then(|s| s.parse::<u8>().ok())
            {
                elements.push(Element::Columns {
                    count: n.clamp(1, 4),
                });
                i += 1;
                continue;
            }
            if let Some(style) = inner
                .strip_prefix("pagenumber:")
                .or_else(|| inner.strip_prefix("page-number:"))
                .map(str::trim)
            {
                let style = match style {
                    "roman" | "rom" => PageNumberStyle::Roman,
                    "none" | "off" | "hide" => PageNumberStyle::None,
                    _ => PageNumberStyle::Arabic,
                };
                elements.push(Element::PageNumberMode { style });
                i += 1;
                continue;
            }
            if let Some(mode) = inner
                .strip_prefix("running-header:")
                .or_else(|| inner.strip_prefix("runningheader:"))
                .map(str::trim)
            {
                let enabled = matches!(mode, "on" | "true" | "yes" | "1");
                elements.push(Element::RunningHeaderMode { enabled });
                i += 1;
                continue;
            }
            if inner == "toc" || inner == "tableofcontents" || inner == "table-of-contents" {
                elements.push(Element::Toc);
                i += 1;
                continue;
            }
            if inner == "bibliography" || inner == "references" {
                elements.push(Element::Bibliography);
                i += 1;
                continue;
            }
        }

        // Image: ![alt](path)
        if trimmed.starts_with("![")
            && let Some(caps) = img_re.captures(trimmed)
        {
            let alt = caps[1].to_string();
            let path = caps[2].to_string();
            elements.push(Element::Image { alt, path });
            i += 1;
            continue;
        }

        // Standalone link line: [text](url) — only if the entire line is a link
        if trimmed.starts_with('[')
            && !trimmed.starts_with("[^")
            && let Some(caps) = link_re.captures(trimmed)
        {
            let text = caps[1].to_string();
            let url = caps[2].to_string();
            elements.push(Element::Link { text, url });
            i += 1;
            continue;
        }

        // Blockquote
        if trimmed.starts_with('>') {
            let mut depth: u8 = 0;
            let mut rest = trimmed;
            while rest.starts_with('>') {
                depth += 1;
                rest = rest[1..].trim_start();
            }
            let text = strip_inline_formatting(rest);
            elements.push(Element::BlockQuote { text, depth });
            i += 1;
            continue;
        }

        // Task list items: - [ ] or - [x]
        if trimmed.starts_with("- [ ] ")
            || trimmed.starts_with("- [x] ")
            || trimmed.starts_with("- [X] ")
        {
            let checked = !trimmed.starts_with("- [ ] ");
            let text = strip_inline_formatting(&trimmed[6..]);
            elements.push(Element::TaskListItem { checked, text });
            i += 1;
            continue;
        }

        // Table rows (contains |)
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            let inner = &trimmed[1..trimmed.len() - 1];
            let cells: Vec<String> = inner.split('|').map(|c| c.trim().to_string()).collect();
            let is_separator = cells.iter().all(|c| {
                let t = c.trim_matches(':').trim();
                !t.is_empty() && t.chars().all(|ch| ch == '-')
            });
            if is_separator {
                let alignments: Vec<TableAlignment> =
                    cells.iter().map(|c| parse_cell_alignment(c)).collect();
                elements.push(Element::TableRow {
                    cells,
                    is_separator: true,
                    alignments,
                    colspans: Vec::new(),
                    rowspans: Vec::new(),
                });
            } else {
                let cells: Vec<String> = cells
                    .into_iter()
                    .map(|c| strip_inline_formatting(&c))
                    .collect();
                let alignments = vec![TableAlignment::Left; cells.len()];
                elements.push(Element::TableRow {
                    cells,
                    is_separator: false,
                    alignments,
                    colspans: Vec::new(),
                    rowspans: Vec::new(),
                });
            }
            i += 1;
            continue;
        }

        // Citation definition: [@key]: full reference
        if trimmed.starts_with("[@")
            && let Some(close) = trimmed.find("]:")
        {
            let key = trimmed[2..close].trim().to_string();
            let text = strip_inline_formatting(trimmed[close + 2..].trim());
            if !key.is_empty() {
                elements.push(Element::CitationDef { key, text });
                i += 1;
                continue;
            }
        }

        // Footnote definition: [^label]: text
        if trimmed.starts_with("[^")
            && let Some(close) = trimmed.find("]:")
        {
            let label = trimmed[2..close].to_string();
            let text = strip_inline_formatting(trimmed[close + 2..].trim());
            elements.push(Element::Footnote { label, text });
            i += 1;
            continue;
        }

        // Definition list: line starting with ": " after a paragraph
        if trimmed.starts_with(": ") {
            let definition = strip_inline_formatting(trimmed.strip_prefix(": ").unwrap_or(trimmed));
            // The term is the previous paragraph element
            let term = match elements.last() {
                Some(Element::Paragraph { text }) => text.clone(),
                _ => String::new(),
            };
            // Remove the paragraph that was actually the term
            if !term.is_empty() {
                elements.pop();
            }
            elements.push(Element::DefinitionItem { term, definition });
            i += 1;
            continue;
        }

        // Unordered list items (detect indentation depth)
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let indent = line.len() - line.trim_start().len();
            let depth = (indent / 2) as u8;
            let text = strip_inline_formatting(&trimmed[2..]);
            elements.push(Element::UnorderedListItem { text, depth });
            i += 1;
            continue;
        }

        // Ordered list items
        if let Some(dot_pos) = trimmed.find(". ") {
            let num_part = &trimmed[..dot_pos];
            if !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit()) {
                let number: u32 = num_part.parse().unwrap_or(1);
                let indent = line.len() - line.trim_start().len();
                let depth = (indent / 3) as u8;
                let text = strip_inline_formatting(&trimmed[dot_pos + 2..]);
                elements.push(Element::OrderedListItem {
                    number,
                    text,
                    depth,
                });
                i += 1;
                continue;
            }
        }

        // Inline math: line that is entirely $expression$ (single dollar)
        if trimmed.starts_with('$')
            && !trimmed.starts_with("$$")
            && trimmed.ends_with('$')
            && trimmed.len() > 2
        {
            let expr = trimmed[1..trimmed.len() - 1].to_string();
            if !expr.is_empty() {
                elements.push(Element::MathInline { expression: expr });
                i += 1;
                continue;
            }
        }

        // Regular paragraph — also strip footnote references [^N] -> (N)
        let trimmed_with_refs = footnote_ref_re.replace_all(trimmed, "($1)").to_string();

        // Check for inline formatting and use RichParagraph if present
        if has_inline_formatting(&trimmed_with_refs) {
            let segments = parse_inline_formatting(&trimmed_with_refs);
            if !segments.is_empty() {
                elements.push(Element::RichParagraph { segments });
            }
        } else {
            let text = strip_inline_formatting(&trimmed_with_refs);
            if !text.is_empty() {
                elements.push(Element::Paragraph { text });
            }
        }
        i += 1;
    }

    // Close unclosed code block
    if in_code_block && !code_buf.is_empty() {
        if let Some(chart) = crate::chart::try_parse_chart_element(&code_lang, &code_buf) {
            elements.push(chart);
        } else {
            elements.push(Element::CodeBlock {
                language: code_lang,
                code: code_buf,
            });
        }
    }

    // Close unclosed math block
    if in_math_block && !math_buf.is_empty() {
        elements.push(Element::MathBlock {
            expression: math_buf,
        });
    }

    elements
}
