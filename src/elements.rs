//! Document element types and Markdown parsing
//!
//! This module defines the core [`Element`] enum used throughout the crate
//! to represent structured document content, along with a [`parse_markdown`]
//! function that converts CommonMark-style Markdown into a vector of
//! elements ready for PDF generation.
//!
//! # Element types
//!
//! | Element | Markdown syntax |
//! |---------|----------------|
//! | `Heading` | `# Title` through `###### Title` |
//! | `Paragraph` | Plain text block |
//! | `UnorderedListItem` | `- item` or `* item` |
//! | `OrderedListItem` | `1. item` |
//! | `CodeBlock` | ` ```lang\ncode\n``` ` |
//! | `BlockQuote` | `> quote` |
//! | `HorizontalRule` | `---` |
//! | `Image` | `![alt](path)` |
//! | `Chart` | ` ```chart bar|line|pie` ` |
//! | `Toc` / page modes | `<!-- toc -->`, `<!-- pagenumber:roman -->`, … |
//! | `CitationDef` | `[@key]: reference` |
//! | `TableRow` | `| col1 | col2 |` |
//! | `MathBlock` / `MathInline` | `$$...$$` / `$...$` |
//!
//! # Example
//!
//! ```rust
//! use pdfrs::elements::parse_markdown;
//!
//! let md = "# Hello\n\nThis is a **bold** paragraph.";
//! let elements = parse_markdown(md);
//! assert!(elements.len() >= 2);
//! ```

/// Structured document elements parsed from Markdown.
/// These carry formatting intent so the PDF generator can render
/// headers at different sizes, indent lists, etc.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TableAlignment {
    Left,
    Center,
    Right,
}

/// Text segment with inline formatting
#[derive(Debug, Clone, PartialEq)]
pub enum TextSegment {
    Plain(String),
    Bold(String),
    Italic(String),
    BoldItalic(String),
    Code(String),
    Strikethrough(String),
    MathInline(String),
    Link { text: String, url: String },
    /// Inline citation `[@key]` rendered as [n].
    Citation { key: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    /// Rich paragraph with multiple styled segments
    RichParagraph { segments: Vec<TextSegment> },
    UnorderedListItem { text: String, depth: u8 },
    OrderedListItem { number: u32, text: String, depth: u8 },
    TaskListItem { checked: bool, text: String },
    CodeBlock { language: String, code: String },
    InlineCode { code: String },
    TableRow { cells: Vec<String>, is_separator: bool, alignments: Vec<TableAlignment> },
    BlockQuote { text: String, depth: u8 },
    DefinitionItem { term: String, definition: String },
    Footnote { label: String, text: String },
    Link { text: String, url: String },
    Image { alt: String, path: String },
    StyledText { text: String, bold: bool, italic: bool },
    MathBlock { expression: String },
    MathInline { expression: String },
    PageBreak,
    HorizontalRule,
    EmptyLine,
    /// Switch multi-column layout for following content (`<!-- columns:N -->`).
    Columns { count: u8 },
    /// Chart from a fenced ` ```chart …` ` block (bar / line / pie).
    Chart {
        kind: ChartKind,
        title: Option<String>,
        /// `(label, value)` pairs
        points: Vec<(String, f32)>,
    },
    /// Page folio style (`<!-- pagenumber:roman|arabic|none -->`).
    PageNumberMode { style: PageNumberStyle },
    /// Running header toggle (`<!-- running-header:on|off -->`).
    RunningHeaderMode { enabled: bool },
    /// Insert an in-document table of contents (`<!-- toc -->`).
    Toc,
    /// Emit numbered bibliography from citations (`<!-- bibliography -->`).
    Bibliography,
    /// Citation definition: `[@key]: full reference text`.
    CitationDef { key: String, text: String },
}

/// Chart type for [`Element::Chart`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Bar,
    Line,
    Pie,
}

/// Page number / folio style for thesis front matter vs body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageNumberStyle {
    #[default]
    Arabic,
    Roman,
    None,
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

/// Parse inline markdown formatting into styled text segments
pub fn parse_inline_formatting(text: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut remaining = text.to_string();

    // Citations [@key] or [@k1; @k2] before links
    let cite_re = regex::Regex::new(r"\[@([^\]]+)\]").unwrap();
    while let Some(caps) = cite_re.captures(&remaining) {
        let full_match = caps.get(0).unwrap();
        let before = &remaining[..full_match.start()];
        let keys = caps.get(1).unwrap().as_str();
        if !before.is_empty() {
            segments.extend(parse_inline_formatting_after_cites(before));
        }
        for key in keys.split(';') {
            let key = key.trim().trim_start_matches('@').trim();
            if !key.is_empty() {
                segments.push(TextSegment::Citation {
                    key: key.to_string(),
                });
            }
        }
        remaining = remaining[full_match.end()..].to_string();
    }

    if !remaining.is_empty() {
        segments.extend(parse_inline_formatting_after_cites(&remaining));
    }

    segments
}

fn parse_inline_formatting_after_cites(text: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut remaining = text.to_string();

    // Links
    let link_re = regex::Regex::new(r"\[([^\]]+)\]\(([^\)]+)\)").unwrap();
    while let Some(caps) = link_re.captures(&remaining) {
        let full_match = caps.get(0).unwrap();
        let before = &remaining[..full_match.start()];
        let link_text = caps.get(1).unwrap().as_str();
        let url = caps.get(2).unwrap().as_str();

        if !before.is_empty() {
            segments.extend(parse_formatting_no_links(before));
        }

        segments.push(TextSegment::Link {
            text: link_text.to_string(),
            url: url.to_string(),
        });
        remaining = remaining[full_match.end()..].to_string();
    }

    if !remaining.is_empty() {
        segments.extend(parse_formatting_no_links(&remaining));
    }

    segments
}

/// Parse formatting excluding links
fn parse_formatting_no_links(text: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut remaining = text.to_string();

    // Inline math (higher priority than bold/italic)
    let math_re = regex::Regex::new(r"\$([^$\n]+)\$").unwrap();
    while let Some(caps) = math_re.captures(&remaining) {
        let full_match = caps.get(0).unwrap();
        let before = &remaining[..full_match.start()];
        let math_expr = caps.get(1).unwrap().as_str();

        if !before.is_empty() {
            segments.extend(parse_code_and_bold_italic(before));
        }

        segments.push(TextSegment::MathInline(math_expr.to_string()));
        remaining = remaining[full_match.end()..].to_string();
    }

    if !remaining.is_empty() {
        segments.extend(parse_code_and_bold_italic(&remaining));
    }

    segments
}

/// Parse code/bold/italic formatting (no links, no math)
fn parse_code_and_bold_italic(text: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut remaining = text.to_string();

    // Strikethrough first so ~~...~~ is not partially eaten by italic rules
    let strike_re = regex::Regex::new(r"~~(.+?)~~").unwrap();
    while let Some(caps) = strike_re.captures(&remaining) {
        let full_match = caps.get(0).unwrap();
        let before = &remaining[..full_match.start()];
        let content = caps.get(1).unwrap().as_str();

        if !before.is_empty() {
            segments.extend(parse_code_and_bold_italic_no_strike(before));
        }
        segments.push(TextSegment::Strikethrough(content.to_string()));
        remaining = remaining[full_match.end()..].to_string();
    }

    if !remaining.is_empty() {
        segments.extend(parse_code_and_bold_italic_no_strike(&remaining));
    }

    segments
}

fn parse_code_and_bold_italic_no_strike(text: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut remaining = text.to_string();

    // Code (high priority)
    let code_re = regex::Regex::new(r"`([^`]+)`").unwrap();
    while let Some(caps) = code_re.captures(&remaining) {
        let full_match = caps.get(0).unwrap();
        let before = &remaining[..full_match.start()];
        let code = caps.get(1).unwrap().as_str();

        if !before.is_empty() {
            segments.extend(parse_bold_italic(before));
        }

        segments.push(TextSegment::Code(code.to_string()));
        remaining = remaining[full_match.end()..].to_string();
    }

    if !remaining.is_empty() {
        segments.extend(parse_bold_italic(&remaining));
    }

    segments
}

/// Parse bold/italic formatting
fn parse_bold_italic(text: &str) -> Vec<TextSegment> {
    if text.is_empty() {
        return Vec::new();
    }

    // Bold+italic: ***text*** or ___text___ (explicit patterns)
    let bi_stars_re = regex::Regex::new(r"\*\*\*(.+?)\*\*\*").unwrap();
    let bi_under_re = regex::Regex::new(r"___(.+?)___").unwrap();
    // Bold: **text** or __text__
    let b_stars_re = regex::Regex::new(r"\*\*(.+?)\*\*").unwrap();
    let b_under_re = regex::Regex::new(r"__(.+?)__").unwrap();
    // Italic: *text* or _text_
    let i_stars_re = regex::Regex::new(r"\*([^*]+)\*").unwrap();
    let i_under_re = regex::Regex::new(r"_([^_]+)_").unwrap();

    #[derive(Clone, Copy)]
    enum Kind {
        BoldItalic,
        Bold,
        Italic,
    }

    let mut best: Option<(usize, usize, Kind, String)> = None;
    let consider = |best: &mut Option<(usize, usize, Kind, String)>,
                    caps: regex::Captures,
                    kind: Kind| {
        let full = caps.get(0).unwrap();
        let content = caps.get(1).unwrap().as_str().to_string();
        let start = full.start();
        let end = full.end();
        match best {
            None => *best = Some((start, end, kind, content)),
            Some((bs, be, _, _)) if start < *bs || (start == *bs && end > *be) => {
                *best = Some((start, end, kind, content));
            }
            _ => {}
        }
    };

    if let Some(caps) = bi_stars_re.captures(text) {
        consider(&mut best, caps, Kind::BoldItalic);
    }
    if let Some(caps) = bi_under_re.captures(text) {
        consider(&mut best, caps, Kind::BoldItalic);
    }
    if let Some(caps) = b_stars_re.captures(text) {
        consider(&mut best, caps, Kind::Bold);
    }
    if let Some(caps) = b_under_re.captures(text) {
        consider(&mut best, caps, Kind::Bold);
    }
    if let Some(caps) = i_stars_re.captures(text) {
        consider(&mut best, caps, Kind::Italic);
    }
    if let Some(caps) = i_under_re.captures(text) {
        consider(&mut best, caps, Kind::Italic);
    }

    let Some((start, end, kind, content)) = best else {
        return vec![TextSegment::Plain(text.to_string())];
    };

    let mut segments = Vec::new();
    if start > 0 {
        segments.extend(parse_bold_italic(&text[..start]));
    }
    segments.push(match kind {
        Kind::BoldItalic => TextSegment::BoldItalic(content),
        Kind::Bold => TextSegment::Bold(content),
        Kind::Italic => TextSegment::Italic(content),
    });
    if end < text.len() {
        segments.extend(parse_bold_italic(&text[end..]));
    }
    segments
}

/// Check if text contains any inline markdown formatting
pub fn has_inline_formatting(text: &str) -> bool {
    text.contains("**")
        || text.contains("__")
        || text.contains("***")
        || text.contains("___")
        || text.contains("~~")
        || text.contains('`')
        || text.contains('[')
        || (text.contains('$') && text.matches('$').count() >= 2)
}

/// Parse markdown text into structured elements
pub fn parse_markdown(markdown: &str) -> Vec<Element> {
    parse_markdown_with_hook(markdown, None)
}

/// Parse Markdown with an optional line-level hook (used by the plugin system).
///
/// The hook runs for each line outside code/math fences. Returning
/// `Some((elements, consumed))` appends those elements and skips `consumed` lines.
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
    let img_re = regex::Regex::new(r"^!\[([^\]]*)\]\(([^\)]+)\)$").unwrap();
    let link_re = regex::Regex::new(r"^\[([^\]]+)\]\(([^\)]+)\)$").unwrap();
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
                elements.push(Element::MathBlock { expression: math_buf.clone() });
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
                if let Some(chart) =
                    crate::chart::try_parse_chart_element(&code_lang, &code_buf)
                {
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
                code_lang = trimmed.strip_prefix("```").unwrap_or(trimmed).trim().to_string();
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
        if trimmed.starts_with("![") {
            if let Some(caps) = img_re.captures(trimmed) {
                let alt = caps[1].to_string();
                let path = caps[2].to_string();
                elements.push(Element::Image { alt, path });
                i += 1;
                continue;
            }
        }

        // Standalone link line: [text](url) — only if the entire line is a link
        if trimmed.starts_with('[') && !trimmed.starts_with("[^") {
            if let Some(caps) = link_re.captures(trimmed) {
                let text = caps[1].to_string();
                let url = caps[2].to_string();
                elements.push(Element::Link { text, url });
                i += 1;
                continue;
            }
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
            && let Some(close) = trimmed.find("]:") {
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
                elements.push(Element::OrderedListItem { number, text, depth });
                i += 1;
                continue;
            }
        }

        // Inline math: line that is entirely $expression$ (single dollar)
        if trimmed.starts_with('$') && !trimmed.starts_with("$$") && trimmed.ends_with('$') && trimmed.len() > 2 {
            let expr = trimmed[1..trimmed.len() - 1].to_string();
            if !expr.is_empty() {
                elements.push(Element::MathInline { expression: expr });
                i += 1;
                continue;
            }
        }

        // Regular paragraph — also strip footnote references [^N] -> (N)
        let footnote_ref_re = regex::Regex::new(r"\[\^([^\]]+)\]").unwrap();
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
        elements.push(Element::MathBlock { expression: math_buf });
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
    fn test_parse_mixed_inline_styles_left_to_right() {
        let segments = parse_inline_formatting(
            "Emphasis: **bold**, *italic*, ***both***, `code`, ~~strike~~",
        );
        assert!(segments.iter().any(|s| matches!(s, TextSegment::Bold(t) if t == "bold")));
        assert!(segments.iter().any(|s| matches!(s, TextSegment::Italic(t) if t == "italic")));
        assert!(segments.iter().any(|s| matches!(s, TextSegment::BoldItalic(t) if t == "both")));
        assert!(segments.iter().any(|s| matches!(s, TextSegment::Code(t) if t == "code")));
        assert!(segments.iter().any(|s| matches!(s, TextSegment::Strikethrough(t) if t == "strike")));
        let joined: String = segments
            .iter()
            .map(|s| match s {
                TextSegment::Plain(t)
                | TextSegment::Bold(t)
                | TextSegment::Italic(t)
                | TextSegment::BoldItalic(t)
                | TextSegment::Strikethrough(t) => t.as_str(),
                TextSegment::Code(t) => t.as_str(),
                _ => "",
            })
            .collect();
        assert!(!joined.contains("**"));
        assert!(!joined.contains("~~"));
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

    #[test]
    fn test_parse_image() {
        let md = "![Logo](images/logo.png)";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0], Element::Image {
            alt: "Logo".into(),
            path: "images/logo.png".into(),
        });
    }

    #[test]
    fn test_parse_image_empty_alt() {
        let md = "![](photo.jpg)";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0], Element::Image {
            alt: "".into(),
            path: "photo.jpg".into(),
        });
    }

    #[test]
    fn test_parse_chart_fence() {
        let md = "```chart pie\ntitle: Mix\nA, 10\nB, 20\n```\n";
        let elements = parse_markdown(md);
        match &elements[0] {
            Element::Chart { kind, title, points } => {
                assert_eq!(*kind, ChartKind::Pie);
                assert_eq!(title.as_deref(), Some("Mix"));
                assert_eq!(points.len(), 2);
            }
            other => panic!("expected Chart, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_thesis_directives_and_citations() {
        let md = "\
<!-- pagenumber:roman -->\n\
<!-- running-header:on -->\n\
<!-- toc -->\n\
See [@smith2020].\n\
<!-- bibliography -->\n\
[@smith2020]: Smith (2020).\n\
<!-- pagenumber:arabic -->\n";
        let elements = parse_markdown(md);
        assert!(elements.iter().any(|e| matches!(e, Element::PageNumberMode { style: PageNumberStyle::Roman })));
        assert!(elements.iter().any(|e| matches!(e, Element::RunningHeaderMode { enabled: true })));
        assert!(elements.iter().any(|e| matches!(e, Element::Toc)));
        assert!(elements.iter().any(|e| matches!(e, Element::Bibliography)));
        assert!(elements.iter().any(|e| matches!(e, Element::CitationDef { key, .. } if key == "smith2020")));
        assert!(elements.iter().any(|e| matches!(e, Element::RichParagraph { segments } if segments.iter().any(|s| matches!(s, TextSegment::Citation { key } if key == "smith2020")))));
        assert!(elements.iter().any(|e| matches!(e, Element::PageNumberMode { style: PageNumberStyle::Arabic })));
    }

    #[test]
    fn test_parse_standalone_link() {
        let md = "[Click here](https://example.com)";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0], Element::Link {
            text: "Click here".into(),
            url: "https://example.com".into(),
        });
    }

    #[test]
    fn test_parse_pagebreak_html() {
        let md = "<!-- pagebreak -->";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0], Element::PageBreak);
    }

    #[test]
    fn test_parse_columns_directive() {
        let md = "<!-- columns:2 -->\n\nHello\n\n<!-- columns:1 -->";
        let elements = parse_markdown(md);
        assert!(elements.iter().any(|e| matches!(e, Element::Columns { count: 2 })));
        assert!(elements.iter().any(|e| matches!(e, Element::Columns { count: 1 })));
    }

    #[test]
    fn test_parse_pagebreak_latex() {
        let md = "\\pagebreak";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0], Element::PageBreak);
    }

    #[test]
    fn test_parse_mixed_new_elements() {
        let md = "# Title\n\n![img](a.png)\n\n[link](http://x.com)\n\n<!-- pagebreak -->\n\nParagraph after break.";
        let elements = parse_markdown(md);
        let types: Vec<&str> = elements.iter().map(|e| match e {
            Element::Heading { .. } => "heading",
            Element::Image { .. } => "image",
            Element::Link { .. } => "link",
            Element::PageBreak => "pagebreak",
            Element::Paragraph { .. } => "paragraph",
            Element::EmptyLine => "empty",
            _ => "other",
        }).collect();
        assert!(types.contains(&"heading"));
        assert!(types.contains(&"image"));
        assert!(types.contains(&"link"));
        assert!(types.contains(&"pagebreak"));
        assert!(types.contains(&"paragraph"));
    }

    #[test]
    fn test_parse_inline_math_inside_paragraph() {
        let md = "Energy formula is $E = mc^2$ in physics.";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);

        match &elements[0] {
            Element::RichParagraph { segments } => {
                assert!(segments.iter().any(|s| matches!(s, TextSegment::MathInline(expr) if expr == "E = mc^2")));
            }
            _ => panic!("Expected RichParagraph with inline math segment"),
        }
    }

    #[test]
    fn test_has_inline_formatting_detects_math() {
        assert!(has_inline_formatting("A math example: $a^2 + b^2 = c^2$"));
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn strip_inline_formatting_plain_text_idempotent(s in "[a-zA-Z0-9 ]{0,500}") {
            // For plain text (no markdown chars), stripping should be idempotent
            let stripped_once = strip_inline_formatting(&s);
            let stripped_twice = strip_inline_formatting(&stripped_once);
            prop_assert_eq!(stripped_once, stripped_twice);
        }

        #[test]
        fn strip_inline_formatting_removes_formatting(s in "\\PC{0,500}") {
            // Stripping should reduce or keep same length
            let original = s.len();
            let stripped = strip_inline_formatting(&s).len();
            prop_assert!(stripped <= original, "Stripping should not increase length");
        }

        #[test]
        fn strip_inline_formatting_doesnt_crash(s in "\\PC{0,500}") {
            // Just ensure we don't panic on any input
            let _ = strip_inline_formatting(&s);
        }

        #[test]
        fn heading_levels_valid(heading in "#[ \t]{0,10}[a-zA-Z0-9 ]{0,100}") {
            let elements = parse_markdown(&heading);
            for elem in elements {
                match elem {
                    Element::Heading { level, .. } => {
                        prop_assert!(level >= 1 && level <= 6, "Heading level must be 1-6");
                    }
                    _ => {}
                }
            }
        }

        #[test]
        fn list_depths_non_negative(list in "-([ \t]{0,10}[a-zA-Z0-9 ]{1,50}){0,5}") {
            let elements = parse_markdown(&list);
            for elem in elements {
                match elem {
                    Element::UnorderedListItem { depth, .. } |
                    Element::OrderedListItem { depth, .. } => {
                        prop_assert!(depth <= 10, "List depth should be reasonable");
                    }
                    _ => {}
                }
            }
        }

        #[test]
        fn empty_markdown_yields_empty(input in "\\PC*") {
            let truncated = if input.len() > 1000 { &input[..1000] } else { &input };
            let trimmed = truncated.trim();
            if trimmed.is_empty() {
                let elements = parse_markdown(trimmed);
                prop_assert_eq!(elements.len(), 0);
            }
        }

        #[test]
        fn double_newline_creates_empty_line(text in "[a-zA-Z0-9 ]{1,50}") {
            let md = format!("{}\n\n{}", text, text);
            let elements = parse_markdown(&md);
            // Should have at least one EmptyLine between paragraphs
            prop_assert!(elements.iter().any(|e| matches!(e, Element::EmptyLine)));
        }

        #[test]
        fn parse_then_strip_does_not_crash(s in "\\PC{0,1000}") {
            let _ = parse_markdown(&s);
            let _ = strip_inline_formatting(&s);
            // Just ensure we don't panic
        }

        #[test]
        fn footnote_definition_has_label(md in "\\[\\^[a-zA-Z0-9_]{1,20}\\]:[ \t]{0,5}[a-zA-Z0-9 ]{0,100}") {
            let elements = parse_markdown(&md);
            if !elements.is_empty() {
                match &elements[0] {
                    Element::Footnote { label, .. } => {
                        prop_assert!(!label.is_empty());
                        prop_assert!(label.len() <= 20);
                    }
                    _ => {}
                }
            }
        }
    }
}
