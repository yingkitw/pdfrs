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

mod inline;
mod parser;
mod types;

pub use inline::{has_inline_formatting, parse_inline_formatting, strip_inline_formatting};
pub use parser::{parse_markdown, parse_markdown_with_hook};
pub use types::{ChartKind, ChartSeries, Element, PageNumberStyle, TableAlignment, TextSegment};

use std::sync::OnceLock;

macro_rules! inline_regex {
    ($name:ident, $pat:literal) => {
        fn $name() -> &'static regex::Regex {
            static RE: OnceLock<regex::Regex> = OnceLock::new();
            RE.get_or_init(|| regex::Regex::new($pat).unwrap())
        }
    };
}

inline_regex!(re_strike, r"~~(.*?)~~");
inline_regex!(re_bold_italic, r"\*\*\*(.*?)\*\*\*");
inline_regex!(re_bold, r"\*\*(.*?)\*\*");
inline_regex!(re_bold2, r"__(.*?)__");
inline_regex!(re_italic, r"\*(.*?)\*");
inline_regex!(re_italic2, r"_(.*?)_");
inline_regex!(re_link_strip, r"\[([^\]]+)\]\([^\)]+\)");
inline_regex!(re_code_strip, r"`([^`]+)`");
inline_regex!(re_cite, r"\[@([^\]]+)\]");
inline_regex!(re_link, r"\[([^\]]+)\]\(([^\)]+)\)");
// Display math first so `$$...$$` is not eaten as `$` + `$...$` + `$`.
inline_regex!(re_display_math, r"\$\$([^$\n]+)\$\$");
inline_regex!(re_math, r"\$([^$\n]+)\$");
inline_regex!(re_strike_parse, r"~~(.+?)~~");
inline_regex!(re_code_parse, r"`([^`]+)`");
inline_regex!(re_bi_stars, r"\*\*\*(.+?)\*\*\*");
inline_regex!(re_bi_under, r"___(.+?)___");
inline_regex!(re_b_stars, r"\*\*(.+?)\*\*");
inline_regex!(re_b_under, r"__(.+?)__");
inline_regex!(re_i_stars, r"\*([^*]+)\*");
inline_regex!(re_i_under, r"_([^_]+)_");

inline_regex!(re_img, r"^!\[([^\]]*)\]\(([^\)]+)\)$");
inline_regex!(re_link_line, r"^\[([^\]]+)\]\(([^\)]+)\)$");
inline_regex!(re_footnote_ref, r"\[\^([^\]]+)\]");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_heading() {
        let elements = parse_markdown("# Hello\n## World");
        assert_eq!(elements.len(), 2);
        assert_eq!(
            elements[0],
            Element::Heading {
                level: 1,
                text: "Hello".into()
            }
        );
        assert_eq!(
            elements[1],
            Element::Heading {
                level: 2,
                text: "World".into()
            }
        );
    }

    #[test]
    fn test_parse_task_list() {
        let elements = parse_markdown("- [ ] Todo\n- [x] Done");
        assert_eq!(elements.len(), 2);
        assert_eq!(
            elements[0],
            Element::TaskListItem {
                checked: false,
                text: "Todo".into()
            }
        );
        assert_eq!(
            elements[1],
            Element::TaskListItem {
                checked: true,
                text: "Done".into()
            }
        );
    }

    #[test]
    fn test_parse_strikethrough() {
        assert_eq!(strip_inline_formatting("~~removed~~"), "removed");
        assert_eq!(
            strip_inline_formatting("keep ~~this~~ text"),
            "keep this text"
        );
    }

    #[test]
    fn test_parse_mixed_inline_styles_left_to_right() {
        let segments =
            parse_inline_formatting("Emphasis: **bold**, *italic*, ***both***, `code`, ~~strike~~");
        assert!(
            segments
                .iter()
                .any(|s| matches!(s, TextSegment::Bold(t) if t == "bold"))
        );
        assert!(
            segments
                .iter()
                .any(|s| matches!(s, TextSegment::Italic(t) if t == "italic"))
        );
        assert!(
            segments
                .iter()
                .any(|s| matches!(s, TextSegment::BoldItalic(t) if t == "both"))
        );
        assert!(
            segments
                .iter()
                .any(|s| matches!(s, TextSegment::Code(t) if t == "code"))
        );
        assert!(
            segments
                .iter()
                .any(|s| matches!(s, TextSegment::Strikethrough(t) if t == "strike"))
        );
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
        assert_eq!(
            elements[0],
            Element::BlockQuote {
                text: "quoted text".into(),
                depth: 1
            }
        );
        assert_eq!(
            elements[1],
            Element::BlockQuote {
                text: "nested".into(),
                depth: 2
            }
        );
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
        assert_eq!(
            elements[0],
            Element::CodeBlock {
                language: "rust".into(),
                code: "fn main() {}".into(),
            }
        );
    }

    #[test]
    fn test_parse_table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 3);
        match &elements[0] {
            Element::TableRow {
                cells,
                is_separator,
                ..
            } => {
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
            Element::TableRow {
                is_separator,
                alignments,
                ..
            } => {
                assert!(is_separator);
                assert_eq!(
                    alignments,
                    &[
                        TableAlignment::Left,
                        TableAlignment::Center,
                        TableAlignment::Right
                    ]
                );
            }
            _ => panic!("Expected separator with alignments"),
        }
    }

    #[test]
    fn test_parse_definition_list() {
        let md = "Term\n: Definition text";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(
            elements[0],
            Element::DefinitionItem {
                term: "Term".into(),
                definition: "Definition text".into(),
            }
        );
    }

    #[test]
    fn test_parse_nested_list() {
        let md = "- Top\n  - Nested\n    - Deep";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 3);
        assert_eq!(
            elements[0],
            Element::UnorderedListItem {
                text: "Top".into(),
                depth: 0
            }
        );
        assert_eq!(
            elements[1],
            Element::UnorderedListItem {
                text: "Nested".into(),
                depth: 1
            }
        );
        assert_eq!(
            elements[2],
            Element::UnorderedListItem {
                text: "Deep".into(),
                depth: 2
            }
        );
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
        assert_eq!(
            elements[0],
            Element::Footnote {
                label: "1".into(),
                text: "This is a footnote.".into(),
            }
        );
    }

    #[test]
    fn test_parse_footnote_reference_in_paragraph() {
        let md = "Some text with a reference[^1].";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::Paragraph { text } => {
                assert!(
                    text.contains("(1)"),
                    "Footnote ref should be converted to (1), got: {}",
                    text
                );
                assert!(
                    !text.contains("[^1]"),
                    "Raw footnote ref should be stripped"
                );
            }
            _ => panic!("Expected Paragraph"),
        }
    }

    #[test]
    fn test_parse_footnote_named_label() {
        let md = "[^note]: A named footnote.";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(
            elements[0],
            Element::Footnote {
                label: "note".into(),
                text: "A named footnote.".into(),
            }
        );
    }

    #[test]
    fn test_parse_image() {
        let md = "![Logo](images/logo.png)";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(
            elements[0],
            Element::Image {
                alt: "Logo".into(),
                path: "images/logo.png".into(),
            }
        );
    }

    #[test]
    fn test_parse_image_empty_alt() {
        let md = "![](photo.jpg)";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(
            elements[0],
            Element::Image {
                alt: "".into(),
                path: "photo.jpg".into(),
            }
        );
    }

    #[test]
    fn test_parse_chart_fence() {
        let md = "```chart pie\ntitle: Mix\nA, 10\nB, 20\n```\n";
        let elements = parse_markdown(md);
        match &elements[0] {
            Element::Chart {
                kind,
                title,
                points,
                series: _,
            } => {
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
        assert!(elements.iter().any(|e| matches!(
            e,
            Element::PageNumberMode {
                style: PageNumberStyle::Roman
            }
        )));
        assert!(
            elements
                .iter()
                .any(|e| matches!(e, Element::RunningHeaderMode { enabled: true }))
        );
        assert!(elements.iter().any(|e| matches!(e, Element::Toc)));
        assert!(elements.iter().any(|e| matches!(e, Element::Bibliography)));
        assert!(
            elements
                .iter()
                .any(|e| matches!(e, Element::CitationDef { key, .. } if key == "smith2020"))
        );
        assert!(elements.iter().any(|e| matches!(e, Element::RichParagraph { segments } if segments.iter().any(|s| matches!(s, TextSegment::Citation { key } if key == "smith2020")))));
        assert!(elements.iter().any(|e| matches!(
            e,
            Element::PageNumberMode {
                style: PageNumberStyle::Arabic
            }
        )));
    }

    #[test]
    fn test_parse_standalone_link() {
        let md = "[Click here](https://example.com)";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        assert_eq!(
            elements[0],
            Element::Link {
                text: "Click here".into(),
                url: "https://example.com".into(),
            }
        );
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
        assert!(
            elements
                .iter()
                .any(|e| matches!(e, Element::Columns { count: 2 }))
        );
        assert!(
            elements
                .iter()
                .any(|e| matches!(e, Element::Columns { count: 1 }))
        );
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
        let types: Vec<&str> = elements
            .iter()
            .map(|e| match e {
                Element::Heading { .. } => "heading",
                Element::Image { .. } => "image",
                Element::Link { .. } => "link",
                Element::PageBreak => "pagebreak",
                Element::Paragraph { .. } => "paragraph",
                Element::EmptyLine => "empty",
                _ => "other",
            })
            .collect();
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
                assert!(
                    segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::MathInline(expr) if expr == "E = mc^2"))
                );
            }
            _ => panic!("Expected RichParagraph with inline math segment"),
        }
    }

    #[test]
    fn test_parse_display_math_dollars_inside_paragraph() {
        let md = "Another block with sets: $$A \\subseteq B \\cup C$$";
        let elements = parse_markdown(md);
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            Element::RichParagraph { segments } => {
                let math = segments.iter().find_map(|s| match s {
                    TextSegment::MathInline(expr) => Some(expr.as_str()),
                    _ => None,
                });
                assert_eq!(math, Some(r"A \subseteq B \cup C"));
                // Must not leave orphan `$` text segments from partial `$...$` matches.
                assert!(
                    !segments
                        .iter()
                        .any(|s| matches!(s, TextSegment::Plain(t) if t.contains('$'))),
                    "unexpected leftover dollars: {:?}",
                    segments
                );
            }
            _ => panic!("Expected RichParagraph, got {:?}", elements[0]),
        }
    }

    #[test]
    fn test_has_inline_formatting_detects_math() {
        assert!(has_inline_formatting("A math example: $a^2 + b^2 = c^2$"));
    }

    #[test]
    fn test_parse_markdown_preserves_soft_hyphen() {
        let md = "co\u{00AD}operate is one word.";
        let elements = parse_markdown(md);
        let text = match &elements[0] {
            Element::Paragraph { text } => text,
            _ => panic!("expected Paragraph, got {:?}", elements[0]),
        };
        assert!(
            text.contains('\u{00AD}'),
            "soft hyphen must survive parse_markdown: {text:?}"
        );
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
                if let Element::Heading { level, .. } = elem {
                    prop_assert!((1..=6).contains(&level), "Heading level must be 1-6");
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
            if !elements.is_empty()
                && let Element::Footnote { label, .. } = &elements[0] {
                    prop_assert!(!label.is_empty());
                    prop_assert!(label.len() <= 20);
                }
        }
    }
}
