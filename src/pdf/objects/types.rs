//! Core PDF object model: [`PdfDocument`], [`PdfObject`], and [`PdfValue`]
//! with document loading, reference rewriting, deduplication, and serialization.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PdfDocument {
    pub version: String,
    pub objects: HashMap<u32, PdfObject>,
    pub catalog: u32,
    pub pages: Vec<u32>,
}

#[derive(Debug, Clone)]
pub enum PdfObject {
    Dictionary(HashMap<String, PdfValue>),
    Stream {
        dictionary: HashMap<String, PdfValue>,
        data: Vec<u8>,
    },
    Array(Vec<PdfValue>),
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
    Reference(u32, u32),
    Name(String),
}

#[derive(Debug, Clone)]
pub enum PdfValue {
    Object(PdfObject),
    Reference(u32, u32),
}

pub(super) fn serialize_value(val: &PdfValue) -> String {
    match val {
        PdfValue::Object(obj) => serialize_object(obj),
        PdfValue::Reference(id, generation) => format!("{} {} R", id, generation),
    }
}

pub(super) fn serialize_object(obj: &PdfObject) -> String {
    match obj {
        PdfObject::Dictionary(dict) => {
            let mut entries: Vec<String> = Vec::new();
            for (key, value) in dict {
                entries.push(format!("/{} {}", key, serialize_value(value)));
            }
            format!("<< {} >>", entries.join(" "))
        }
        PdfObject::Stream { dictionary, data } => {
            let mut entries: Vec<String> = Vec::new();
            for (key, value) in dictionary {
                entries.push(format!("/{} {}", key, serialize_value(value)));
            }
            format!(
                "<< {} >>\nstream\n{}\nendstream",
                entries.join(" "),
                String::from_utf8_lossy(data)
            )
        }
        PdfObject::Array(items) => {
            let parts: Vec<String> = items.iter().map(serialize_value).collect();
            format!("[ {} ]", parts.join(" "))
        }
        PdfObject::String(s) => s.clone(),
        PdfObject::Number(n) => {
            if *n == (n.round()) {
                format!("{:.0}", n)
            } else {
                n.to_string()
            }
        }
        PdfObject::Boolean(b) => b.to_string(),
        PdfObject::Null => "null".to_string(),
        PdfObject::Reference(id, generation) => format!("{} {} R", id, generation),
        PdfObject::Name(n) => format!("/{}", n),
    }
}

// --- Document implementation ---

impl Default for PdfDocument {
    fn default() -> Self {
        Self::new()
    }
}
