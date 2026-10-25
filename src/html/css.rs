//! CSS parsing for HTML-to-PDF: stylesheet + inline styles, selectors, cascade.

use super::tokenizer::get_attr;

#[derive(Debug, Clone, Default)]
pub(super) struct ComputedStyle {
    pub(super) bold: bool,
    pub(super) italic: bool,
    /// CSS color value (e.g. `#ff0000`, `red`).
    pub(super) color: Option<String>,
    /// CSS background-color value.
    pub(super) background_color: Option<String>,
    /// CSS font-size value (e.g. `14px`, `1.5em`).
    pub(super) font_size: Option<String>,
    /// CSS text-align value.
    pub(super) text_align: Option<String>,
    /// CSS margin values.
    pub(super) margin: Option<String>,
    /// CSS padding values.
    pub(super) padding: Option<String>,
    /// CSS border shorthand.
    pub(super) border: Option<String>,
}

/// A CSS rule with a selector and declared properties.
#[derive(Debug, Clone)]
pub(super) struct CssRule {
    pub(super) selector: String,
    pub(super) style: ComputedStyle,
}

/// Parse a CSS stylesheet string into a list of rules.
pub(super) fn parse_stylesheet(css: &str) -> Vec<CssRule> {
    let mut rules = Vec::new();
    let chars: Vec<char> = css.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Skip whitespace and comments.
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if i >= chars.len() {
            break;
        }
        // Read selector until `{`.
        let sel_start = i;
        while i < chars.len() && chars[i] != '{' {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let selector: String = chars[sel_start..i]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        i += 1; // skip {
        // Read declarations until `}`.
        let decl_start = i;
        while i < chars.len() && chars[i] != '}' {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let declarations: String = chars[decl_start..i].iter().collect();
        i += 1; // skip }
        let style = parse_declarations(&declarations);
        if !selector.is_empty() {
            rules.push(CssRule { selector, style });
        }
    }
    rules
}

/// Parse CSS declarations like `font-weight: bold; color: red;` into a ComputedStyle.
fn parse_declarations(decls: &str) -> ComputedStyle {
    let mut style = ComputedStyle::default();
    for decl in decls.split(';') {
        let decl = decl.trim();
        if let Some((prop, val)) = decl.split_once(':') {
            let prop = prop.trim().to_lowercase();
            let val = val.trim().to_string();
            match prop.as_str() {
                "font-weight" => {
                    style.bold = val == "bold" || val == "700" || val == "800" || val == "900";
                }
                "font-style" => {
                    style.italic = val == "italic" || val == "oblique";
                }
                "color" => style.color = Some(val),
                "background-color" | "background" => style.background_color = Some(val),
                "font-size" => style.font_size = Some(val),
                "text-align" => style.text_align = Some(val),
                "margin" => style.margin = Some(val),
                "padding" => style.padding = Some(val),
                "border" => style.border = Some(val),
                _ => {}
            }
        }
    }
    style
}

/// Parse an inline `style` attribute value into a ComputedStyle.
fn parse_inline_style(style_str: &str) -> ComputedStyle {
    parse_declarations(style_str)
}

/// Match a CSS selector against an HTML element tag and class.
/// Supports tag selectors (`p`, `h1`), class selectors (`.classname`),
/// and tag.class combinations (`p.highlight`).
fn matches_selector(selector: &str, tag: &str, attrs: &[(String, String)]) -> bool {
    let selector = selector.trim();
    if selector.is_empty() {
        return false;
    }
    // Check for class selector (`.classname` or `tag.classname`).
    if let Some(dot_pos) = selector.find('.') {
        let sel_tag = &selector[..dot_pos];
        let sel_class = &selector[dot_pos + 1..];
        if !sel_tag.is_empty() && sel_tag.to_lowercase() != tag {
            return false;
        }
        // Check if element has this class.
        let class_val = get_attr(attrs, "class").unwrap_or("");
        return class_val.split_whitespace().any(|c| c == sel_class);
    }
    // Check for id selector (`#id`).
    if let Some(hash_pos) = selector.find('#') {
        let sel_tag = &selector[..hash_pos];
        let sel_id = &selector[hash_pos + 1..];
        if !sel_tag.is_empty() && sel_tag.to_lowercase() != tag {
            return false;
        }
        let id_val = get_attr(attrs, "id").unwrap_or("");
        return id_val == sel_id;
    }
    // Plain tag selector.
    selector.to_lowercase() == tag
}

/// Compute the style for an element by applying matching CSS rules and inline style.
pub(super) fn compute_style(
    tag: &str,
    attrs: &[(String, String)],
    rules: &[CssRule],
) -> ComputedStyle {
    let mut style = ComputedStyle::default();
    // Apply matching stylesheet rules in order.
    for rule in rules {
        if matches_selector(&rule.selector, tag, attrs) {
            merge_style(&mut style, &rule.style);
        }
    }
    // Apply inline style (highest priority).
    if let Some(inline) = get_attr(attrs, "style") {
        let inline_style = parse_inline_style(inline);
        merge_style(&mut style, &inline_style);
    }
    style
}

/// Merge `src` into `dst`, with `src` taking priority for set fields.
pub(super) fn merge_style(dst: &mut ComputedStyle, src: &ComputedStyle) {
    if src.bold {
        dst.bold = true;
    }
    if src.italic {
        dst.italic = true;
    }
    if src.color.is_some() {
        dst.color = src.color.clone();
    }
    if src.background_color.is_some() {
        dst.background_color = src.background_color.clone();
    }
    if src.font_size.is_some() {
        dst.font_size = src.font_size.clone();
    }
    if src.text_align.is_some() {
        dst.text_align = src.text_align.clone();
    }
    if src.margin.is_some() {
        dst.margin = src.margin.clone();
    }
    if src.padding.is_some() {
        dst.padding = src.padding.clone();
    }
    if src.border.is_some() {
        dst.border = src.border.clone();
    }
}
