//! HTML tokenizer: tag/attr/text parsing, entity decoding, simplified DOM.

#[derive(Debug, Clone)]
pub(super) enum Node {
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

pub(super) fn tokenize(html: &str) -> Vec<Node> {
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
pub(super) fn get_attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}
