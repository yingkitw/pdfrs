//! PDF sanitization and JavaScript sandboxing: strips dangerous actions,
//! launch behavior, and `javascript:` URIs; reports what was removed.

use super::objects::{PdfDocument, PdfObject, PdfValue};
use crate::error::Result;
use std::collections::HashMap;

impl PdfDocument {
    pub fn sanitize(&mut self) {
        // Pass 1: identify dangerous object IDs
        let ids_to_remove: Vec<u32> = self
            .objects
            .iter()
            .filter(|(_, obj)| Self::object_is_dangerous(obj))
            .map(|(id, _)| *id)
            .collect();

        // Remove standalone dangerous objects (JS scripts, etc.)
        for id in &ids_to_remove {
            self.objects.remove(id);
        }

        // Pass 2: strip dangerous keys from remaining dictionaries and streams
        for (_, obj) in self.objects.iter_mut() {
            match obj {
                PdfObject::Dictionary(dict) => Self::strip_dangerous_keys(dict),
                PdfObject::Stream { dictionary, .. } => Self::strip_dangerous_keys(dictionary),
                _ => {}
            }
        }

        // Also strip dangerous keys from the catalog
        if let Some(PdfObject::Dictionary(catalog_dict)) = self.objects.get_mut(&self.catalog) {
            catalog_dict.remove("OpenAction");
            catalog_dict.remove("AA");
            catalog_dict.remove("JavaScript");
            catalog_dict.remove("JS");
        }
    }

    /// Check if a PdfObject is inherently dangerous and should be removed entirely.
    fn object_is_dangerous(obj: &PdfObject) -> bool {
        let mut content = String::new();
        match obj {
            PdfObject::Dictionary(dict)
            | PdfObject::Stream {
                dictionary: dict, ..
            } => {
                for (k, v) in dict {
                    content.push_str(k);
                    content.push(' ');
                    content.push_str(&Self::value_to_string(v));
                    content.push(' ');
                }
            }
            PdfObject::String(s) => content.push_str(s),
            _ => {}
        }
        // Standalone JavaScript script objects
        if let PdfObject::Dictionary(dict) = obj
            && (dict.contains_key("JS") || dict.contains_key("JavaScript"))
        {
            return true;
        }
        // Launch actions or embedded malicious scripts in string form
        if content.contains("/S /Launch") || content.contains("/Launch") {
            return true;
        }
        false
    }

    /// Recursively convert a PdfValue to a string for scanning.
    fn value_to_string(val: &PdfValue) -> String {
        match val {
            PdfValue::Object(obj) => match obj {
                PdfObject::String(s) => s.clone(),
                PdfObject::Number(n) => n.to_string(),
                PdfObject::Boolean(b) => b.to_string(),
                PdfObject::Name(n) => format!("/ {}", n),
                PdfObject::Reference(id, generation) => format!("{} {} R", id, generation),
                PdfObject::Null => "null".to_string(),
                PdfObject::Array(arr) => {
                    let parts: Vec<String> = arr.iter().map(Self::value_to_string).collect();
                    format!("[ {} ]", parts.join(" "))
                }
                PdfObject::Dictionary(dict) => {
                    let parts: Vec<String> = dict
                        .iter()
                        .map(|(k, v)| format!("/{} {}", k, Self::value_to_string(v)))
                        .collect();
                    format!("<< {} >>", parts.join(" "))
                }
                PdfObject::Stream { dictionary, .. } => {
                    let parts: Vec<String> = dictionary
                        .iter()
                        .map(|(k, v)| format!("/{} {}", k, Self::value_to_string(v)))
                        .collect();
                    format!("<< {} >>", parts.join(" "))
                }
            },
            PdfValue::Reference(id, generation) => format!("{} {} R", id, generation),
        }
    }

    /// Strip dangerous keys from a dictionary in-place.
    fn strip_dangerous_keys(dict: &mut HashMap<String, PdfValue>) {
        let dangerous_keys: Vec<String> = dict
            .keys()
            .filter(|k| {
                let lower = k.to_lowercase();
                lower == "js" ||
                lower == "javascript" ||
                lower == "launch" ||
                lower == "aa" ||
                // Only remove /F from non-Filespec contexts (filespec needs /F for filename)
                // We check the whole dict to see if it's a Filespec
                (lower == "f" && !dict.contains_key("Type"))
            })
            .cloned()
            .collect();

        for key in dangerous_keys {
            dict.remove(&key);
        }

        // Recursively sanitize nested dictionaries
        for val in dict.values_mut() {
            if let PdfValue::Object(PdfObject::Dictionary(inner)) = val {
                Self::strip_dangerous_keys(inner);
            }
            if let PdfValue::Object(PdfObject::Stream { dictionary, .. }) = val {
                Self::strip_dangerous_keys(dictionary);
            }
            if let PdfValue::Object(PdfObject::Array(arr)) = val {
                for item in arr.iter_mut() {
                    if let PdfValue::Object(PdfObject::Dictionary(inner)) = item {
                        Self::strip_dangerous_keys(inner);
                    }
                    if let PdfValue::Object(PdfObject::Stream { dictionary, .. }) = item {
                        Self::strip_dangerous_keys(dictionary);
                    }
                }
            }
        }
    }

    /// Scan this document for JavaScript actions without modifying it.
    pub fn detect_javascript_actions(&self) -> JavaScriptSandboxReport {
        let mut actions = Vec::new();

        for (id, obj) in &self.objects {
            Self::scan_object_for_javascript(*id, obj, &mut actions);
        }

        if let Some(PdfObject::Dictionary(catalog)) = self.objects.get(&self.catalog) {
            if catalog.contains_key("JavaScript") || catalog.contains_key("JS") {
                actions.push(JavaScriptAction {
                    object_id: Some(self.catalog),
                    kind: "document_script".to_string(),
                    description: "Catalog contains document-level JavaScript".to_string(),
                });
            }
            if catalog.contains_key("OpenAction") {
                actions.push(JavaScriptAction {
                    object_id: Some(self.catalog),
                    kind: "open_action".to_string(),
                    description: "Catalog OpenAction may execute on document open".to_string(),
                });
            }
        }

        JavaScriptSandboxReport {
            actions_found: actions.clone(),
            actions_removed: 0,
            clean: actions.is_empty(),
        }
    }

    /// Neutralize JavaScript actions and return a report of what was found.
    ///
    /// Extends [`sanitize`](Self::sanitize) by also stripping annotation `/A` and `/AA`
    /// dictionaries that execute JavaScript, `javascript:` URI actions, and
    /// document-level `/Names /JavaScript` entries.
    pub fn sandbox(&mut self) -> JavaScriptSandboxReport {
        let before = self.detect_javascript_actions();
        let found = before.actions_found.len();

        self.sanitize();
        self.strip_javascript_action_dictionaries();

        let after = self.detect_javascript_actions();
        JavaScriptSandboxReport {
            actions_found: before.actions_found,
            actions_removed: found.saturating_sub(after.actions_found.len()),
            clean: after.actions_found.is_empty(),
        }
    }

    fn scan_object_for_javascript(id: u32, obj: &PdfObject, actions: &mut Vec<JavaScriptAction>) {
        match obj {
            PdfObject::Dictionary(dict) => Self::scan_dict_for_javascript(Some(id), dict, actions),
            PdfObject::Stream { dictionary, .. } => {
                Self::scan_dict_for_javascript(Some(id), dictionary, actions)
            }
            PdfObject::String(content) => {
                Self::scan_string_for_javascript(Some(id), content, actions)
            }
            _ => {}
        }
    }

    fn scan_dict_for_javascript(
        id: Option<u32>,
        dict: &HashMap<String, PdfValue>,
        actions: &mut Vec<JavaScriptAction>,
    ) {
        if Self::dict_is_javascript_action(dict) {
            actions.push(JavaScriptAction {
                object_id: id,
                kind: "javascript_action".to_string(),
                description: "Dictionary executes JavaScript (/S /JavaScript or /JS)".to_string(),
            });
        }

        for (key, val) in dict {
            if key.eq_ignore_ascii_case("URI") && Self::value_contains_javascript_uri(val) {
                actions.push(JavaScriptAction {
                    object_id: id,
                    kind: "javascript_uri".to_string(),
                    description: "URI action uses a javascript: URL".to_string(),
                });
            }

            match val {
                PdfValue::Object(PdfObject::Dictionary(inner)) => {
                    Self::scan_dict_for_javascript(id, inner, actions);
                }
                PdfValue::Object(PdfObject::Stream { dictionary, .. }) => {
                    Self::scan_dict_for_javascript(id, dictionary, actions);
                }
                PdfValue::Object(PdfObject::String(s)) => {
                    Self::scan_string_for_javascript(id, s, actions);
                }
                PdfValue::Object(PdfObject::Array(arr)) => {
                    for item in arr {
                        if let PdfValue::Object(PdfObject::Dictionary(inner)) = item {
                            Self::scan_dict_for_javascript(id, inner, actions);
                        } else if let PdfValue::Object(PdfObject::String(s)) = item {
                            Self::scan_string_for_javascript(id, s, actions);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn scan_string_for_javascript(
        id: Option<u32>,
        content: &str,
        actions: &mut Vec<JavaScriptAction>,
    ) {
        let lower = content.to_ascii_lowercase();
        if lower.contains("/s /javascript")
            || lower.contains("/js")
            || (lower.contains("javascript") && lower.contains("app."))
        {
            actions.push(JavaScriptAction {
                object_id: id,
                kind: "javascript_action".to_string(),
                description: "String-encoded PDF object contains JavaScript action".to_string(),
            });
        }
        if lower.contains("javascript:") {
            actions.push(JavaScriptAction {
                object_id: id,
                kind: "javascript_uri".to_string(),
                description: "String-encoded object contains javascript: URI".to_string(),
            });
        }
    }

    fn dict_is_javascript_action(dict: &HashMap<String, PdfValue>) -> bool {
        if dict.contains_key("JS") || dict.contains_key("JavaScript") {
            return true;
        }
        dict.get("S").is_some_and(
            |v| matches!(v, PdfValue::Object(PdfObject::String(s)) if s.contains("JavaScript")),
        )
    }

    fn value_contains_javascript_uri(val: &PdfValue) -> bool {
        match val {
            PdfValue::Object(PdfObject::String(s)) => {
                s.to_ascii_lowercase().contains("javascript:")
            }
            _ => false,
        }
    }

    fn strip_javascript_action_dictionaries(&mut self) {
        for obj in self.objects.values_mut() {
            match obj {
                PdfObject::Dictionary(dict) => Self::strip_js_from_dict(dict),
                PdfObject::Stream { dictionary, .. } => Self::strip_js_from_dict(dictionary),
                PdfObject::String(content)
                    if content.to_ascii_lowercase().contains("javascript:") =>
                {
                    *content = Self::neutralize_javascript_uris(content);
                }
                _ => {}
            }
        }
    }

    fn strip_js_from_dict(dict: &mut HashMap<String, PdfValue>) {
        let action_keys: Vec<String> = dict
            .iter()
            .filter(|(key, val)| {
                matches!(key.as_str(), "A" | "AA")
                    && matches!(val, PdfValue::Object(PdfObject::Dictionary(inner)) if Self::dict_is_javascript_action(inner))
            })
            .map(|(k, _)| k.clone())
            .collect();

        for key in action_keys {
            dict.remove(&key);
        }

        let uri_keys: Vec<String> = dict
            .iter()
            .filter(|(key, val)| {
                key.eq_ignore_ascii_case("URI") && Self::value_contains_javascript_uri(val)
            })
            .map(|(k, _)| k.clone())
            .collect();

        for key in uri_keys {
            dict.remove(&key);
        }

        for val in dict.values_mut() {
            match val {
                PdfValue::Object(PdfObject::Dictionary(inner)) => Self::strip_js_from_dict(inner),
                PdfValue::Object(PdfObject::Stream { dictionary, .. }) => {
                    Self::strip_js_from_dict(dictionary)
                }
                PdfValue::Object(PdfObject::Array(arr)) => {
                    for item in arr.iter_mut() {
                        if let PdfValue::Object(PdfObject::Dictionary(inner)) = item {
                            Self::strip_js_from_dict(inner);
                        }
                    }
                }
                PdfValue::Object(PdfObject::String(s))
                    if s.to_ascii_lowercase().contains("javascript:") =>
                {
                    *s = Self::neutralize_javascript_uris(s);
                }
                _ => {}
            }
        }
    }

    fn neutralize_javascript_uris(content: &str) -> String {
        content.replace("javascript:", "blocked:")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaScriptAction {
    pub object_id: Option<u32>,
    pub kind: String,
    pub description: String,
}

/// Report from scanning or sandboxing JavaScript actions in a PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaScriptSandboxReport {
    pub actions_found: Vec<JavaScriptAction>,
    pub actions_removed: usize,
    pub clean: bool,
}

/// Load PDF bytes, sandbox JavaScript actions, and return the cleaned PDF plus report.
pub fn sandbox_pdf_bytes(data: &[u8]) -> Result<(Vec<u8>, JavaScriptSandboxReport)> {
    let mut doc = PdfDocument::load_from_bytes(data)?;
    let report = doc.sandbox();
    Ok((doc.to_bytes(), report))
}
