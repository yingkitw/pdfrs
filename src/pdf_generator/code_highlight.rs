use super::Color;
use syntect::parsing::{SyntaxReference, SyntaxSet};

// Lazy static syntax set and theme
fn get_syntax_set() -> &'static SyntaxSet {
    use std::sync::OnceLock;
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
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
        _ => syntax_set.find_syntax_by_token("Plain Text"),
    }
}

/// Simple syntax token for rendering (reserved for future use)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(super) struct CodeToken {
    pub(super) text: String,
    pub(super) color: Color,
}

/// Perform simple syntax highlighting on code
pub(super) fn highlight_code(code: &str, language: &str) -> Vec<CodeToken> {
    let syntax_set = get_syntax_set();

    let _syntax = get_syntax_for_language(language)
        .unwrap_or_else(|| syntax_set.find_syntax_by_token("Plain Text").unwrap());

    // Use a simple approach - return tokens with different colors
    // This is a simplified version; full syntect integration would be more complex
    let mut tokens = Vec::new();

    // Basic keyword highlighting for common languages
    let keywords = match language.to_lowercase().as_str() {
        "rust" | "rs" => vec![
            "fn", "let", "mut", "pub", "struct", "enum", "impl", "use", "mod",
            "return", "if", "else", "match", "for", "while", "loop", "break", "continue",
            "true", "false", "const", "static", "trait", "type", "where", "move",
            "crate", "ref", "self", "Self", "super", "async", "await", "unsafe",
        ],
        "python" | "py" => vec![
            "def", "class", "if", "else", "elif", "for", "while", "return",
            "import", "from", "as", "try", "except", "finally", "with", "lambda",
            "True", "False", "None", "and", "or", "not", "in", "is", "pass", "break", "continue",
        ],
        "javascript" | "js" | "typescript" | "ts" => vec![
            "function", "const", "let", "var", "if", "else", "for", "while", "return",
            "import", "export", "default", "from", "as", "class", "extends", "new",
            "true", "false", "null", "undefined", "async", "await", "try", "catch", "finally",
            "typeof", "instanceof", "this", "super",
        ],
        _ => vec![],
    };

    let string_color = Color::rgb(0.15, 0.49, 0.07); // Green for strings
    let keyword_color = Color::rgb(0.53, 0.07, 0.24); // Purple for keywords
    let comment_color = Color::rgb(0.4, 0.4, 0.4); // Gray for comments
    let number_color = Color::rgb(0.15, 0.15, 0.8); // Blue for numbers
    let default_color = Color::black();

    // Simple tokenization - split by common patterns
    let mut remaining = code.to_string();

    while !remaining.is_empty() {
        // Check for string literals
        if remaining.starts_with('"') {
            if let Some(end) = remaining[1..].find('"') {
                let token = &remaining[..end + 2];
                tokens.push(CodeToken {
                    text: token.to_string(),
                    color: string_color,
                });
                remaining = remaining[end + 2..].to_string();
                continue;
            }
        }

        // Check for single quotes
        if remaining.starts_with('\'') {
            if let Some(end) = remaining[1..].find('\'') {
                let token = &remaining[..end + 2];
                tokens.push(CodeToken {
                    text: token.to_string(),
                    color: string_color,
                });
                remaining = remaining[end + 2..].to_string();
                continue;
            }
        }

        // Check for comments
        if remaining.starts_with("//") {
            if let Some(end) = remaining.find('\n') {
                let token = &remaining[..end];
                tokens.push(CodeToken {
                    text: token.to_string(),
                    color: comment_color,
                });
                remaining = remaining[end..].to_string();
                continue;
            } else {
                tokens.push(CodeToken {
                    text: remaining.clone(),
                    color: comment_color,
                });
                break;
            }
        }

        // Check for comments (hash style)
        if remaining.starts_with('#') {
            if let Some(end) = remaining.find('\n') {
                let token = &remaining[..end];
                tokens.push(CodeToken {
                    text: token.to_string(),
                    color: comment_color,
                });
                remaining = remaining[end..].to_string();
                continue;
            } else {
                tokens.push(CodeToken {
                    text: remaining.clone(),
                    color: comment_color,
                });
                break;
            }
        }

        // Check for numbers
        if remaining
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            let end = remaining
                .chars()
                .position(|c| !c.is_ascii_digit() && c != '.')
                .unwrap_or(remaining.len());
            let token = &remaining[..end];
            tokens.push(CodeToken {
                text: token.to_string(),
                color: number_color,
            });
            remaining = remaining[end..].to_string();
            continue;
        }

        // Check for keywords
        let mut found_keyword = false;
        for keyword in &keywords {
            if remaining.starts_with(keyword) {
                let next_char = remaining.chars().nth(keyword.len());
                if next_char
                    .map(|c| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(true)
                {
                    tokens.push(CodeToken {
                        text: keyword.to_string(),
                        color: keyword_color,
                    });
                    remaining = remaining[keyword.len()..].to_string();
                    found_keyword = true;
                    break;
                }
            }
        }

        if found_keyword {
            continue;
        }

        // Take a run of plain characters (identifiers, whitespace, punctuation)
        // until we hit something that could start a special token
        let mut end = 0;
        let mut chars_iter = remaining.chars();
        while let Some(c) = chars_iter.next() {
            let rest = &remaining[end..];
            // Stop if we see the start of a string, comment, number-at-word-boundary, or keyword
            if end > 0
                && (c == '"'
                    || c == '\''
                    || rest.starts_with("//")
                    || (c == '#'
                        && !remaining[..end]
                            .ends_with(|ch: char| ch.is_alphanumeric() || ch == '_'))
                    || (c.is_ascii_digit()
                        && (end == 0
                            || !remaining
                                .as_bytes()
                                .get(end.wrapping_sub(1))
                                .map(|b| b.is_ascii_alphanumeric() || *b == b'_')
                                .unwrap_or(false))))
            {
                break;
            }
            // Check if a keyword starts here (only at word boundary)
            let mut is_keyword_start = false;
            if end > 0 {
                let prev = remaining.as_bytes()[end - 1];
                if !prev.is_ascii_alphanumeric() && prev != b'_' {
                    for keyword in &keywords {
                        if rest.starts_with(keyword) {
                            let next = rest.chars().nth(keyword.len());
                            if next
                                .map(|nc| !nc.is_alphanumeric() && nc != '_')
                                .unwrap_or(true)
                            {
                                is_keyword_start = true;
                                break;
                            }
                        }
                    }
                }
            }
            if is_keyword_start {
                break;
            }
            end += c.len_utf8();
        }
        if end == 0 {
            // Couldn't group, take one character
            let c = remaining.chars().next().unwrap();
            end = c.len_utf8();
        }
        let chunk = &remaining[..end];
        tokens.push(CodeToken {
            text: chunk.to_string(),
            color: default_color,
        });
        remaining = remaining[end..].to_string();
    }

    // If tokenization failed, just return the whole code as one token
    if tokens.is_empty() && !code.is_empty() {
        tokens.push(CodeToken {
            text: code.to_string(),
            color: default_color,
        });
    }

    tokens
}
