//! Simple syntax highlighting for code blocks via syntect.

use super::Color;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

fn get_syntax_set() -> &'static SyntaxSet {
    use std::sync::OnceLock;
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn get_theme_set() -> &'static ThemeSet {
    use std::sync::OnceLock;
    static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

fn get_syntax_for_language(lang: &str) -> Option<&'static SyntaxReference> {
    let syntax_set = get_syntax_set();
    match lang.to_lowercase().as_str() {
        "rust" | "rs" => syntax_set.find_syntax_by_token("Rust"),
        "python" | "py" => syntax_set.find_syntax_by_token("Python"),
        "javascript" | "js" => syntax_set.find_syntax_by_token("JavaScript"),
        "typescript" | "ts" => syntax_set.find_syntax_by_token("TypeScript"),
        "html" | "htm" => syntax_set.find_syntax_by_token("HTML"),
        "css" => syntax_set.find_syntax_by_token("CSS"),
        "json" => syntax_set.find_syntax_by_token("JSON"),
        "c" | "cpp" | "cxx" => syntax_set.find_syntax_by_token("C++"),
        "java" => syntax_set.find_syntax_by_token("Java"),
        "go" => syntax_set.find_syntax_by_token("Go"),
        "ruby" => syntax_set.find_syntax_by_token("Ruby"),
        "php" => syntax_set.find_syntax_by_token("PHP"),
        "shell" | "bash" | "sh" => syntax_set.find_syntax_by_token("Bash"),
        "sql" => syntax_set.find_syntax_by_token("SQL"),
        "markdown" | "md" => syntax_set.find_syntax_by_token("Markdown"),
        "xml" => syntax_set.find_syntax_by_token("XML"),
        "yaml" | "yml" => syntax_set.find_syntax_by_token("YAML"),
        "" => None,
        other => syntax_set.find_syntax_by_token(other),
    }
}

fn style_to_color(style: Style) -> Color {
    Color::rgb(
        style.foreground.r as f32 / 255.0,
        style.foreground.g as f32 / 255.0,
        style.foreground.b as f32 / 255.0,
    )
}

/// Highlighted span for PDF code rendering.
#[derive(Debug, Clone)]
pub(super) struct CodeToken {
    pub(super) text: String,
    pub(super) color: Color,
}

/// Highlight a single code line for `language` using syntect.
///
/// Callers typically pass one display line at a time. Falls back to plain
/// dark text when the language is unknown or highlighting fails.
pub(super) fn highlight_code(code: &str, language: &str) -> Vec<CodeToken> {
    if code.is_empty() {
        return Vec::new();
    }
    let syntax_set = get_syntax_set();
    let Some(syntax) = get_syntax_for_language(language) else {
        return plain_tokens(code);
    };
    let themes = get_theme_set();
    let theme = themes
        .themes
        .get("InspiredGitHub")
        .or_else(|| themes.themes.values().next());
    let Some(theme) = theme else {
        return plain_tokens(code);
    };

    let mut highlighter = HighlightLines::new(syntax, theme);
    // syntect expects a trailing newline for line-oriented highlighting.
    let line = if code.ends_with('\n') {
        code.to_string()
    } else {
        format!("{}\n", code)
    };
    match highlighter.highlight_line(&line, syntax_set) {
        Ok(ranges) => {
            let mut tokens = Vec::new();
            for (style, text) in ranges {
                let text = text.trim_end_matches(['\n', '\r']);
                if text.is_empty() {
                    continue;
                }
                tokens.push(CodeToken {
                    text: text.to_string(),
                    color: style_to_color(style),
                });
            }
            if tokens.is_empty() {
                plain_tokens(code)
            } else {
                tokens
            }
        }
        Err(_) => plain_tokens(code),
    }
}

fn plain_tokens(code: &str) -> Vec<CodeToken> {
    vec![CodeToken {
        text: code.to_string(),
        color: Color::rgb(0.15, 0.15, 0.15),
    }]
}
