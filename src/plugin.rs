//! Plugin system for extending Markdown parsing and PDF element generation.
//!
//! Register [`ParserPlugin`]s to recognize custom Markdown syntax and
//! [`GeneratorPlugin`]s to transform [`Element`]s before PDF rendering.
//!
//! # Example
//!
//! ```rust
//! use pdfrs::plugin::{CalloutPlugin, PluginRegistry, parse_markdown_with_plugins};
//!
//! let mut registry = PluginRegistry::new();
//! registry.register_parser(CalloutPlugin);
//!
//! let md = ":::note\nHello callout\n:::\n";
//! let elements = parse_markdown_with_plugins(md, &registry);
//! assert!(!elements.is_empty());
//! ```

use crate::elements::{Element, parse_markdown_with_hook};
use std::sync::Arc;

/// Context for a parser plugin attempting to consume markdown lines.
#[derive(Debug, Clone, Copy)]
pub struct ParseContext<'a> {
    /// Full line slice (original, may include indentation).
    pub line: &'a str,
    /// Trimmed line.
    pub trimmed: &'a str,
    /// Zero-based index of `line` within `lines`.
    pub line_index: usize,
    /// All markdown lines.
    pub lines: &'a [&'a str],
}

/// Parses custom Markdown constructs into document [`Element`]s.
pub trait ParserPlugin: Send + Sync {
    fn name(&self) -> &str;

    /// If this plugin handles the content starting at `ctx.line_index`, return
    /// the produced elements and the number of lines consumed (≥ 1).
    fn try_parse(&self, ctx: &ParseContext<'_>) -> Option<(Vec<Element>, usize)>;
}

/// Transforms elements after parsing, before PDF generation.
pub trait GeneratorPlugin: Send + Sync {
    fn name(&self) -> &str;

    /// Return `Some(replacement)` to replace `element`, or `None` to keep it.
    fn transform_element(&self, element: &Element) -> Option<Vec<Element>>;
}

/// Registry of parser and generator plugins.
#[derive(Clone, Default)]
pub struct PluginRegistry {
    parsers: Vec<Arc<dyn ParserPlugin>>,
    generators: Vec<Arc<dyn GeneratorPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty registry (same as [`new`](Self::new)).
    pub fn empty() -> Self {
        Self::new()
    }

    /// Built-in defaults: [`CalloutPlugin`] as both parser and generator.
    pub fn with_defaults() -> Self {
        let mut r = Self::new();
        r.register_parser(CalloutPlugin);
        r.register_generator(CalloutPlugin);
        r
    }

    pub fn register_parser<P: ParserPlugin + 'static>(&mut self, plugin: P) -> &mut Self {
        self.parsers.push(Arc::new(plugin));
        self
    }

    pub fn register_generator<G: GeneratorPlugin + 'static>(&mut self, plugin: G) -> &mut Self {
        self.generators.push(Arc::new(plugin));
        self
    }

    pub fn parser_names(&self) -> Vec<&str> {
        self.parsers.iter().map(|p| p.name()).collect()
    }

    pub fn generator_names(&self) -> Vec<&str> {
        self.generators.iter().map(|g| g.name()).collect()
    }

    pub fn has_parsers(&self) -> bool {
        !self.parsers.is_empty()
    }

    pub fn has_generators(&self) -> bool {
        !self.generators.is_empty()
    }

    /// Try each parser plugin in registration order.
    pub fn try_parse_line(&self, ctx: &ParseContext<'_>) -> Option<(Vec<Element>, usize)> {
        for plugin in &self.parsers {
            if let Some(result) = plugin.try_parse(ctx) {
                return Some(result);
            }
        }
        None
    }

    /// Apply all generator plugins in order (each sees the output of the previous).
    pub fn apply_generators(&self, elements: Vec<Element>) -> Vec<Element> {
        if self.generators.is_empty() {
            return elements;
        }
        let mut current = elements;
        for plugin in &self.generators {
            let mut next = Vec::with_capacity(current.len());
            for el in &current {
                if let Some(replacement) = plugin.transform_element(el) {
                    next.extend(replacement);
                } else {
                    next.push(el.clone());
                }
            }
            current = next;
        }
        current
    }
}

/// Parse Markdown using the given plugin registry.
///
/// Parser plugins run before built-in syntax (when not inside code/math blocks).
/// Generator plugins run after the full element list is produced.
pub fn parse_markdown_with_plugins(markdown: &str, registry: &PluginRegistry) -> Vec<Element> {
    let elements = if registry.has_parsers() {
        let hook = |lines: &[&str], i: usize| {
            let line = lines[i];
            let trimmed = line.trim();
            let ctx = ParseContext {
                line,
                trimmed,
                line_index: i,
                lines,
            };
            registry.try_parse_line(&ctx)
        };
        parse_markdown_with_hook(markdown, Some(&hook))
    } else {
        parse_markdown_with_hook(markdown, None)
    };
    registry.apply_generators(elements)
}

/// Built-in plugin: fenced callouts (`:::note` / `:::warning` / `:::tip` / `:::danger`).
///
/// ```text
/// :::note
/// Remember to save.
/// :::
/// ```
///
/// Parses to a block quote prefixed with the callout kind. The generator pass
/// expands those quotes into a bold heading line plus body paragraph.
#[derive(Debug, Clone, Copy, Default)]
pub struct CalloutPlugin;

const CALLOUT_KINDS: &[&str] = &["note", "warning", "tip", "danger", "info"];

impl CalloutPlugin {
    fn parse_kind(trimmed: &str) -> Option<&str> {
        let rest = trimmed.strip_prefix(":::")?;
        let kind = rest.trim().to_ascii_lowercase();
        CALLOUT_KINDS.iter().copied().find(|k| *k == kind.as_str())
    }

    fn label_for(kind: &str) -> &'static str {
        match kind {
            "note" | "info" => "NOTE",
            "warning" => "WARNING",
            "tip" => "TIP",
            "danger" => "DANGER",
            _ => "NOTE",
        }
    }
}

impl ParserPlugin for CalloutPlugin {
    fn name(&self) -> &str {
        "callout"
    }

    fn try_parse(&self, ctx: &ParseContext<'_>) -> Option<(Vec<Element>, usize)> {
        let kind = Self::parse_kind(ctx.trimmed)?;
        let label = Self::label_for(kind);
        let mut body_lines = Vec::new();
        let mut consumed = 1;
        let mut j = ctx.line_index + 1;
        while j < ctx.lines.len() {
            let t = ctx.lines[j].trim();
            if t == ":::" {
                consumed += 1;
                break;
            }
            body_lines.push(t);
            consumed += 1;
            j += 1;
        }
        let body = body_lines.join(" ").trim().to_string();
        let text = if body.is_empty() {
            format!("[{}]", label)
        } else {
            format!("[{}] {}", label, body)
        };
        Some((
            vec![Element::BlockQuote { text, depth: 1 }],
            consumed.max(1),
        ))
    }
}

impl GeneratorPlugin for CalloutPlugin {
    fn name(&self) -> &str {
        "callout"
    }

    fn transform_element(&self, element: &Element) -> Option<Vec<Element>> {
        let Element::BlockQuote { text, depth } = element else {
            return None;
        };
        // Match "[NOTE] body" / "[WARNING] body" produced by the parser pass
        let (label, rest) = match CALLOUT_KINDS.iter().find_map(|k| {
            let label = CalloutPlugin::label_for(k);
            let prefix = format!("[{}]", label);
            text.strip_prefix(&prefix).map(|r| (label, r.trim()))
        }) {
            Some(v) => v,
            None => return None,
        };

        let mut out = vec![Element::Paragraph {
            text: format!("▶ {}", label),
        }];
        if !rest.is_empty() {
            out.push(Element::BlockQuote {
                text: rest.to_string(),
                depth: *depth,
            });
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_callout_parser() {
        let mut registry = PluginRegistry::new();
        registry.register_parser(CalloutPlugin);
        let md = ":::warning\nBe careful\n:::\n\nAfter\n";
        let elements = parse_markdown_with_plugins(md, &registry);
        assert!(
            elements.iter().any(|e| matches!(
                e,
                Element::Paragraph { text } if text.contains("WARNING")
            )) || elements.iter().any(|e| matches!(
                e,
                Element::BlockQuote { text, .. } if text.contains("careful") || text.contains("WARNING")
            )),
            "got {:?}",
            elements
        );
    }

    #[test]
    fn test_registry_names() {
        let registry = PluginRegistry::with_defaults();
        assert!(registry.parser_names().contains(&"callout"));
        assert!(registry.generator_names().contains(&"callout"));
    }

    #[test]
    fn test_empty_registry_matches_plain_parse() {
        let md = "# Title\n\nHello";
        let plain = crate::elements::parse_markdown(md);
        let with_empty = parse_markdown_with_plugins(md, &PluginRegistry::empty());
        assert_eq!(plain, with_empty);
    }

    #[test]
    fn test_custom_parser_plugin() {
        struct HashBangPlugin;
        impl ParserPlugin for HashBangPlugin {
            fn name(&self) -> &str {
                "hashbang"
            }
            fn try_parse(&self, ctx: &ParseContext<'_>) -> Option<(Vec<Element>, usize)> {
                if ctx.trimmed.starts_with("#!") {
                    Some((
                        vec![Element::Paragraph {
                            text: format!("META: {}", &ctx.trimmed[2..]),
                        }],
                        1,
                    ))
                } else {
                    None
                }
            }
        }

        let mut registry = PluginRegistry::new();
        registry.register_parser(HashBangPlugin);
        let elements = parse_markdown_with_plugins("#!author=Ada\n\nBody\n", &registry);
        assert!(
            elements
                .iter()
                .any(|e| matches!(e, Element::Paragraph { text } if text.contains("META:") && text.contains("author=Ada"))),
            "got {:?}",
            elements
        );
    }
}
