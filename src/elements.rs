/// Structured document elements parsed from Markdown.
/// These carry formatting intent so the PDF generator can render
/// headers at different sizes, indent lists, etc.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TableAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    UnorderedListItem { text: String, depth: u8 },
    OrderedListItem { number: u32, text: String, depth: u8 },
    TaskListItem { checked: bool, text: String },
    CodeBlock { language: String, code: String },
    TableRow { cells: Vec<String>, is_separator: bool, alignments: Vec<TableAlignment> },
    BlockQuote { text: String, depth: u8 },
    DefinitionItem { term: String, definition: String },
    Footnote { label: String, text: String },
    HorizontalRule,
    EmptyLine,
}

/// Parse alignment from a table separator cell like `:---`, `:---:`, `---:`
fn parse_cell_alignment(cell: &str) -> TableAlignment {
    let t = cell.trim();
    let starts = t.starts_with(':');
    let ends = t.ends_with(':');
    if starts && ends {
        TableAlignment::Center
    } else if ends {
        TableAlignment::Right
    } else {
        TableAlignment::Left
    }
}

/// Strip inline markdown formatting from text (bold, italic, code, links, strikethrough)
pub fn strip_inline_formatting(text: &str) -> String {
    let mut s = text.to_string();

    // Strikethrough ~~text~~
    let strike_re = regex::Regex::new(r"~~(.*?)~~").unwrap();
    s = strike_re.replace_all(&s, "$1").to_string();

    // Bold+italic (***text***)
    let bold_italic_re = regex::Regex::new(r"\*\*\*(.*?)\*\*\*").unwrap();
    s = bold_italic_re.replace_all(&s, "$1").to_string();

    // Bold (**text**)
    let bold_re = regex::Regex::new(r"\*\*(.*?)\*\*").unwrap();
    s = bold_re.replace_all(&s, "$1").to_string();

    // Bold (__text__)
    let bold_re2 = regex::Regex::new(r"__(.*?)__").unwrap();
    s = bold_re2.replace_all(&s, "$1").to_string();

    // Italic (*text*)
    let italic_re = regex::Regex::new(r"\*(.*?)\*").unwrap();
    s = italic_re.replace_all(&s, "$1").to_string();

    // Italic (_text_)
    let italic_re2 = regex::Regex::new(r"_(.*?)_").unwrap();
    s = italic_re2.replace_all(&s, "$1").to_string();

    // Links [text](url)
    let link_re = regex::Regex::new(r"\[([^\]]+)\]\([^\)]+\)").unwrap();
    s = link_re.replace_all(&s, "$1").to_string();

    // Inline code `code`
    let code_re = regex::Regex::new(r"`([^`]+)`").unwrap();
    s = code_re.replace_all(&s, "$1").to_string();

    s
}

/// Parse markdown text into structured elements
pub fn parse_markdown(markdown: &str) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_buf = String::new();
    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Code block toggle
        if trimmed.starts_with("```") {
            if in_code_block {
                elements.push(Element::CodeBlock {
                    language: code_lang.clone(),
                    code: code_buf.clone(),
                });
                code_buf.clear();
                code_lang.clear();
                in_code_block = false;
            } else {
                in_code_block = true;
                code_lang = trimmed[3..].trim().to_string();
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

        // Empty line
        if trimmed.is_empty() {
            elements.push(Element::EmptyLine);
            i += 1;
            continue;
        }

        // Horizontal rule
        if (trimmed == "---" || trimmed == "***" || trimmed == "___")
            && trimmed.len() >= 3
        {
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
        if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
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
                let alignments: Vec<TableAlignment> = cells.iter().map(|c| parse_cell_alignment(c)).collect();
                elements.push(Element::TableRow { cells, is_separator: true, alignments });
            } else {
                let cells: Vec<String> = cells.into_iter().map(|c| strip_inline_formatting(&c)).collect();
                let alignments = vec![TableAlignment::Left; cells.len()];
                elements.push(Element::TableRow { cells, is_separator: false, alignments });
            }
            i += 1;
            continue;
        }

        // Footnote definition: [^label]: text
        if trimmed.starts_with("[^") {
            if let Some(close) = trimmed.find("]:") {
                let label = trimmed[2..close].to_string();
                let text = strip_inline_formatting(trimmed[close + 2..].trim());
                elements.push(Element::Footnote { label, text });
                i += 1;
                continue;
            }
        }

        // Definition list: line starting with ": " after a paragraph
        if trimmed.starts_with(": ") {
            let definition = strip_inline_formatting(&trimmed[2..]);
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
                elements.push(Element::OrderedListItem { number, text, depth });
                i += 1;
                continue;
            }
        }

        // Regular paragraph — also strip footnote references [^N] -> (N)
        let footnote_ref_re = regex::Regex::new(r"\[\^([^\]]+)\]").unwrap();
        let trimmed_with_refs = footnote_ref_re.replace_all(trimmed, "($1)").to_string();
        let text = strip_inline_formatting(&trimmed_with_refs);
        if !text.is_empty() {
            elements.push(Element::Paragraph { text });
        }
        i += 1;
    }

    // Close unclosed code block
    if in_code_block && !code_buf.is_empty() {
        elements.push(Element::CodeBlock {
            language: code_lang,
            code: code_buf,
        });
    }

    elements
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_heading() {
        let elements = parse_markdown("# Hello\n## World");
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0], Element::Heading { level: 1, text: "Hello".into() });
        assert_eq!(elements[1], Element::Heading { level: 2, text: "World".into() });
    }

    #[test]
    fn test_parse_task_list() {
        let elements = parse_markdown("- [ ] Todo\n- [x] Done");
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0], Element::TaskListItem { checked: false, text: "Todo".into() });
        assert_eq!(elements[1], Element::TaskListItem { checked: true, text: "Done".into() });
    }

    #[test]
    fn test_parse_strikethrough() {
        assert_eq!(strip_inline_formatting("~~removed~~"), "removed");
        assert_eq!(strip_inline_formatting("keep ~~this~~ text"), "keep this text");
    }

    #[test]
    fn test_parse_blockquote() {
        let elements = parse_markdown("> quoted text\n>> nested");
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0], Element::BlockQuote { text: "quoted text".into(), depth: 1 });
        assert_eq!(elements[1], Element::BlockQuote { text: "nested".into(), depth: 2 });
    }

    #[test]
    fn test_parse_horizontal_rule() {
        let elements = parse_markdown("---");
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0], Element::HorizontalRule);
    }

    #[test]
    fn test_parse_code_block() {
        let md = "```rust\nfn main() {}\n```";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0], Element::CodeBlock {
            language: "rust".into(),
            code: "fn main() {}".into(),
        });
    }

    #[test]
    fn test_parse_table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 3);
        match &elements[0] {
            Element::TableRow { cells, is_separator, .. } => {
                assert!(!is_separator);
                assert_eq!(cells, &["A", "B"]);
            }
            _ => panic!("Expected TableRow"),
        }
        match &elements[1] {
            Element::TableRow { is_separator, .. } => assert!(is_separator),
            _ => panic!("Expected separator"),
        }
    }

    #[test]
    fn test_parse_table_alignment() {
        let md = "| L | C | R |\n|:---|:---:|---:|\n| a | b | c |";
        let elements = parse_markdown(md);
        match &elements[1] {
            Element::TableRow { is_separator, alignments, .. } => {
                assert!(is_separator);
                assert_eq!(alignments, &[TableAlignment::Left, TableAlignment::Center, TableAlignment::Right]);
            }
            _ => panic!("Expected separator with alignments"),
        }
    }

    #[test]
    fn test_parse_definition_list() {
        let md = "Term\n: Definition text";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0], Element::DefinitionItem {
            term: "Term".into(),
            definition: "Definition text".into(),
        });
    }

    #[test]
    fn test_parse_nested_list() {
        let md = "- Top\n  - Nested\n    - Deep";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 3);
        assert_eq!(elements[0], Element::UnorderedListItem { text: "Top".into(), depth: 0 });
        assert_eq!(elements[1], Element::UnorderedListItem { text: "Nested".into(), depth: 1 });
        assert_eq!(elements[2], Element::UnorderedListItem { text: "Deep".into(), depth: 2 });
    }

    #[test]
    fn test_strip_inline_formatting() {
        assert_eq!(strip_inline_formatting("**bold**"), "bold");
        assert_eq!(strip_inline_formatting("*italic*"), "italic");
        assert_eq!(strip_inline_formatting("`code`"), "code");
        assert_eq!(strip_inline_formatting("[link](http://x.com)"), "link");
        assert_eq!(strip_inline_formatting("***both***"), "both");
    }

    #[test]
    fn test_parse_footnote_definition() {
        let md = "[^1]: This is a footnote.";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0], Element::Footnote {
            label: "1".into(),
            text: "This is a footnote.".into(),
        });
    }

    #[test]
    fn test_parse_footnote_reference_in_paragraph() {
        let md = "Some text with a reference[^1].";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::Paragraph { text } => {
                assert!(text.contains("(1)"), "Footnote ref should be converted to (1), got: {}", text);
                assert!(!text.contains("[^1]"), "Raw footnote ref should be stripped");
            }
            _ => panic!("Expected Paragraph"),
        }
    }

    #[test]
    fn test_parse_footnote_named_label() {
        let md = "[^note]: A named footnote.";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0], Element::Footnote {
            label: "note".into(),
            text: "A named footnote.".into(),
        });
    }
}
