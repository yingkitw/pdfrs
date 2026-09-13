//! Minimal XML parser for SVG documents.

use super::svg_document::{SvgElement, SvgNode};
use crate::error::{PdfError, Result};
use std::collections::HashMap;

// ----- Minimal XML parser for SVG ----------------------------------------

pub(super) fn parse_svg_xml(src: &str) -> Result<SvgElement> {
    let mut parser = SvgXmlParser::new(src);
    parser.skip_prolog();
    let root = parser.parse_element()?;
    Ok(root)
}

struct SvgXmlParser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> SvgXmlParser<'a> {
    fn new(src: &'a str) -> Self {
        SvgXmlParser { src, pos: 0 }
    }

    fn skip_prolog(&mut self) {
        self.skip_ws();
        if self.src[self.pos..].starts_with("<?xml")
            && let Some(end) = self.src[self.pos..].find("?>")
        {
            self.pos += end + 2;
        }
        if self.src[self.pos..].starts_with("<!--")
            && let Some(end) = self.src[self.pos..].find("-->")
        {
            self.pos += end + 3;
        }
        // Skip DOCTYPE if present.
        if self.src[self.pos..]
            .to_ascii_uppercase()
            .starts_with("<!doctype")
            && let Some(end) = self.src[self.pos..].find('>')
        {
            self.pos += end + 1;
        }
    }

    fn skip_ws(&mut self) {
        let bytes = self.src.as_bytes();
        while self.pos < bytes.len() {
            let c = bytes[self.pos] as char;
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn parse_element(&mut self) -> Result<SvgElement> {
        self.skip_ws();
        let bytes = self.src.as_bytes();
        if self.pos >= bytes.len() || bytes[self.pos] as char != '<' {
            return Err(PdfError::Svg("expected '<' at element start".into()));
        }
        self.pos += 1; // consume '<'
        // Self-closing or end tag.
        if self.peek() == Some('/') {
            return Err(PdfError::Svg("unexpected closing tag".into()));
        }
        let name = self.parse_name();
        let mut attrs = HashMap::new();
        loop {
            self.skip_ws();
            if self.peek() == Some('/') {
                self.pos += 1;
                if self.peek() == Some('>') {
                    self.pos += 1;
                }
                return Ok(SvgElement {
                    name,
                    attrs,
                    children: Vec::new(),
                });
            }
            if self.peek() == Some('>') {
                self.pos += 1;
                break;
            }
            // Parse attribute.
            let key = self.parse_name();
            if key.is_empty() {
                // Skip unrecognized character.
                self.pos += 1;
                continue;
            }
            self.skip_ws();
            if self.peek() == Some('=') {
                self.pos += 1;
                self.skip_ws();
                let value = self.parse_attr_value();
                attrs.insert(key, value);
            } else {
                attrs.insert(key.clone(), String::new());
            }
        }
        // Parse children until matching close tag.
        let mut children = Vec::new();
        loop {
            self.skip_ws();
            if self.pos >= self.src.len() {
                break;
            }
            let rest = &self.src[self.pos..];
            if rest.starts_with("</") {
                // Closing tag.
                self.pos += 2;
                let close_name = self.parse_name();
                self.skip_ws();
                if self.peek() == Some('>') {
                    self.pos += 1;
                }
                let _ = close_name; // best-effort: don't strictly verify name
                break;
            }
            if rest.starts_with("<!--") {
                if let Some(end) = rest.find("-->") {
                    self.pos += end + 3;
                    continue;
                } else {
                    break;
                }
            }
            if rest.starts_with("<![CDATA[") {
                if let Some(end) = rest.find("]]>") {
                    let text = &rest[9..end];
                    if !text.trim().is_empty() {
                        children.push(SvgNode::Text(text.to_string()));
                    }
                    self.pos += end + 3;
                    continue;
                } else {
                    break;
                }
            }
            if rest.starts_with('<') {
                let child = self.parse_element();
                if let Ok(c) = child {
                    children.push(SvgNode::Element(c));
                } else {
                    // Skip malformed element.
                    if let Some(end) = rest.find('>') {
                        self.pos += end + 1;
                    } else {
                        break;
                    }
                }
            } else {
                // Text node — collect until next '<'.
                let end = rest.find('<').unwrap_or(rest.len());
                let text = &rest[..end];
                if !text.trim().is_empty() {
                    children.push(SvgNode::Text(decode_entities(text)));
                }
                self.pos += end;
            }
        }
        Ok(SvgElement {
            name,
            attrs,
            children,
        })
    }

    fn parse_name(&mut self) -> String {
        let bytes = self.src.as_bytes();
        let start = self.pos;
        while self.pos < bytes.len() {
            let c = bytes[self.pos] as char;
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ':' || c == '.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.src[start..self.pos].to_string()
    }

    fn parse_attr_value(&mut self) -> String {
        let bytes = self.src.as_bytes();
        if self.pos >= bytes.len() {
            return String::new();
        }
        let quote = bytes[self.pos] as char;
        if quote != '"' && quote != '\'' {
            // Unquoted value until whitespace or '>'.
            let start = self.pos;
            while self.pos < bytes.len() {
                let c = bytes[self.pos] as char;
                if c.is_whitespace() || c == '>' || c == '/' {
                    break;
                }
                self.pos += 1;
            }
            return self.src[start..self.pos].to_string();
        }
        self.pos += 1;
        let start = self.pos;
        while self.pos < bytes.len() && bytes[self.pos] as char != quote {
            self.pos += 1;
        }
        let value = &self.src[start..self.pos];
        if self.pos < bytes.len() {
            self.pos += 1; // closing quote
        }
        decode_entities(value)
    }
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}
