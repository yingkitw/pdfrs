//! Inline Markdown formatting: bold/italic/code/links/math/citation segments.
use super::{
    TextSegment, re_b_stars, re_b_under, re_bi_stars, re_bi_under, re_bold, re_bold_italic,
    re_bold2, re_cite, re_code_parse, re_code_strip, re_i_stars, re_i_under, re_italic, re_italic2,
    re_link, re_link_strip, re_math, re_strike, re_strike_parse,
};

pub fn strip_inline_formatting(text: &str) -> String {
    let mut s = text.to_string();

    // Strikethrough ~~text~~
    s = re_strike().replace_all(&s, "$1").to_string();

    // Bold+italic (***text***)
    s = re_bold_italic().replace_all(&s, "$1").to_string();

    // Bold (**text**)
    s = re_bold().replace_all(&s, "$1").to_string();

    // Bold (__text__)
    s = re_bold2().replace_all(&s, "$1").to_string();

    // Italic (*text*)
    s = re_italic().replace_all(&s, "$1").to_string();

    // Italic (_text_)
    s = re_italic2().replace_all(&s, "$1").to_string();

    // Links [text](url)
    s = re_link_strip().replace_all(&s, "$1").to_string();

    // Inline code `code`
    s = re_code_strip().replace_all(&s, "$1").to_string();

    s
}

/// Parse inline markdown formatting into styled text segments
pub fn parse_inline_formatting(text: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut remaining = text.to_string();

    // Citations [@key] or [@k1; @k2] before links
    while let Some(caps) = re_cite().captures(&remaining) {
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

pub(super) fn parse_inline_formatting_after_cites(text: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut remaining = text.to_string();

    // Links
    while let Some(caps) = re_link().captures(&remaining) {
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
    while let Some(caps) = re_math().captures(&remaining) {
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
    while let Some(caps) = re_strike_parse().captures(&remaining) {
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
    while let Some(caps) = re_code_parse().captures(&remaining) {
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
    // Bold: **text** or __text__
    // Italic: *text* or _text_

    #[derive(Clone, Copy)]
    enum Kind {
        BoldItalic,
        Bold,
        Italic,
    }

    let mut best: Option<(usize, usize, Kind, String)> = None;
    let consider =
        |best: &mut Option<(usize, usize, Kind, String)>, caps: regex::Captures, kind: Kind| {
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

    if let Some(caps) = re_bi_stars().captures(text) {
        consider(&mut best, caps, Kind::BoldItalic);
    }
    if let Some(caps) = re_bi_under().captures(text) {
        consider(&mut best, caps, Kind::BoldItalic);
    }
    if let Some(caps) = re_b_stars().captures(text) {
        consider(&mut best, caps, Kind::Bold);
    }
    if let Some(caps) = re_b_under().captures(text) {
        consider(&mut best, caps, Kind::Bold);
    }
    if let Some(caps) = re_i_stars().captures(text) {
        consider(&mut best, caps, Kind::Italic);
    }
    if let Some(caps) = re_i_under().captures(text) {
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
