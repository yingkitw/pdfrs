//! Academic thesis helpers: page folios, TOC lines, and citation numbering.
//!
//! Markdown switches (HTML comments):
//! - `<!-- pagenumber:roman|arabic|none -->`
//! - `<!-- running-header:on|off -->`
//! - `<!-- toc -->`
//! - `<!-- bibliography -->`
//!
//! Citations use Pandoc-like `[@key]` in prose and `[@key]: full reference`
//! definition lines. Figures/tables are numbered automatically during layout.

use crate::elements::{Element, PageNumberStyle};
use crate::pdf_generator::OutlineDest;
use std::collections::HashMap;

/// Convert `n` (1-based) to lowercase Roman numerals (thesis front-matter style).
pub fn to_roman(mut n: u32) -> String {
    if n == 0 {
        return "0".into();
    }
    let table: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (val, sym) in table {
        while n >= val {
            out.push_str(sym);
            n -= val;
        }
    }
    out
}

/// Format a displayed page folio for the given style and 1-based counter.
pub fn format_folio(style: PageNumberStyle, n: u32) -> Option<String> {
    match style {
        PageNumberStyle::None => None,
        PageNumberStyle::Arabic => Some(n.to_string()),
        PageNumberStyle::Roman => Some(to_roman(n)),
    }
}

/// Build TOC body elements from collected outline destinations.
pub fn build_toc_elements(outlines: &[OutlineDest]) -> Vec<Element> {
    let mut out = Vec::new();
    out.push(Element::Heading {
        level: 1,
        text: "Contents".into(),
    });
    out.push(Element::EmptyLine);
    if outlines.is_empty() {
        out.push(Element::Paragraph {
            text: "(No headings found.)".into(),
        });
        return out;
    }
    for dest in outlines {
        // Skip the synthetic "Contents" heading itself if present.
        if dest.level == 1 && dest.title.eq_ignore_ascii_case("Contents") {
            continue;
        }
        let indent = "  ".repeat(dest.level.saturating_sub(1) as usize);
        let dots = ".";
        let label = if dest.page_label.is_empty() {
            (dest.page_index + 1).to_string()
        } else {
            dest.page_label.clone()
        };
        // Leader dots between title and page label.
        let title = dest.title.trim();
        let core = format!("{}{}", indent, title);
        let pad = (48usize.saturating_sub(core.chars().count())).max(3);
        let line = format!(
            "{} {}{} {}",
            core,
            dots.repeat(pad / dots.len()),
            dots,
            label
        );
        out.push(Element::Paragraph { text: line });
    }
    out.push(Element::EmptyLine);
    out
}

/// Replace each [`Element::Toc`] with generated TOC paragraphs using `outlines`.
///
/// When `outline_filter` is provided, only destinations whose titles appear in
/// that allow-list (post-TOC headings) are included.
pub fn expand_toc(elements: &[Element], outlines: &[OutlineDest]) -> Vec<Element> {
    // Titles of headings that occur after the first Toc marker.
    let mut after_toc = false;
    let mut allowed: Vec<String> = Vec::new();
    for el in elements {
        match el {
            Element::Toc => after_toc = true,
            Element::Heading { text, .. } if after_toc => allowed.push(text.clone()),
            _ => {}
        }
    }
    let filtered: Vec<OutlineDest> = if allowed.is_empty() {
        outlines.to_vec()
    } else {
        outlines
            .iter()
            .filter(|o| allowed.iter().any(|t| t == &o.title))
            .cloned()
            .collect()
    };
    let toc_body = build_toc_elements(&filtered);
    let mut out = Vec::with_capacity(elements.len() + toc_body.len());
    for el in elements {
        match el {
            Element::Toc => out.extend(toc_body.iter().cloned()),
            other => out.push(other.clone()),
        }
    }
    out
}

/// Collect `[@key]: text` definitions from the document.
pub fn collect_citation_defs(elements: &[Element]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for el in elements {
        if let Element::CitationDef { key, text } = el {
            map.insert(key.clone(), text.clone());
        }
    }
    map
}

/// Assign stable citation numbers in order of first appearance in `keys`.
#[derive(Debug, Default, Clone)]
pub struct CitationRegistry {
    order: Vec<String>,
    numbers: HashMap<String, u32>,
}

impl CitationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the 1-based number for `key`, allocating on first use.
    pub fn number_for(&mut self, key: &str) -> u32 {
        if let Some(n) = self.numbers.get(key) {
            return *n;
        }
        let n = (self.order.len() as u32) + 1;
        self.order.push(key.to_string());
        self.numbers.insert(key.to_string(), n);
        n
    }

    pub fn ordered_keys(&self) -> &[String] {
        &self.order
    }
}

/// Build bibliography list elements from registry + definitions.
pub fn build_bibliography_elements(
    registry: &CitationRegistry,
    defs: &HashMap<String, String>,
) -> Vec<Element> {
    let mut out = Vec::new();
    out.push(Element::Heading {
        level: 1,
        text: "Bibliography".into(),
    });
    out.push(Element::EmptyLine);
    if registry.ordered_keys().is_empty() {
        out.push(Element::Paragraph {
            text: "(No citations.)".into(),
        });
        return out;
    }
    for key in registry.ordered_keys() {
        let n = registry.numbers.get(key).copied().unwrap_or(0);
        let text = defs
            .get(key)
            .cloned()
            .unwrap_or_else(|| format!("[{}]", key));
        out.push(Element::Paragraph {
            text: format!("[{}] {}", n, text),
        });
    }
    out.push(Element::EmptyLine);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman_numerals() {
        assert_eq!(to_roman(1), "i");
        assert_eq!(to_roman(4), "iv");
        assert_eq!(to_roman(9), "ix");
        assert_eq!(to_roman(14), "xiv");
        assert_eq!(to_roman(27), "xxvii");
    }

    #[test]
    fn citation_registry_stable() {
        let mut r = CitationRegistry::new();
        assert_eq!(r.number_for("smith"), 1);
        assert_eq!(r.number_for("jones"), 2);
        assert_eq!(r.number_for("smith"), 1);
        assert_eq!(
            r.ordered_keys(),
            &["smith".to_string(), "jones".to_string()]
        );
    }
}
