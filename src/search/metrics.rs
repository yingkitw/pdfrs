//! Font metrics collection (embedded + base-14 width tables) for text layout.
use super::pages::{as_array, as_ref_id, object_dict};
use crate::pdf::{PdfDocument, PdfObject, PdfValue};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FontMetrics {
    widths: HashMap<u32, u16>,
    default_width: u16,
}

impl FontMetrics {
    pub fn advance(&self, ch: u32) -> u16 {
        self.widths.get(&ch).copied().unwrap_or(self.default_width)
    }
}

pub fn collect_font_metrics(doc: &PdfDocument) -> HashMap<String, FontMetrics> {
    let mut out = HashMap::new();
    // Scan every object for a /Resources dictionary (pages, catalog, and
    // standalone resource dicts alike), so per-page fonts are covered.
    for obj in doc.objects.values() {
        let dict = match obj {
            PdfObject::Dictionary(d) => d,
            PdfObject::Stream { dictionary, .. } => dictionary,
            _ => continue,
        };
        if let Some(resources) = dict.get("Resources") {
            walk_resources(doc, resources, &mut out);
        }
    }
    out
}

fn walk_resources(doc: &PdfDocument, val: &PdfValue, out: &mut HashMap<String, FontMetrics>) {
    let Some(dict) = (match val {
        PdfValue::Object(PdfObject::Dictionary(d)) => Some(d),
        PdfValue::Reference(id, _) => object_dict(doc, *id),
        _ => None,
    }) else {
        return;
    };
    if let Some(fonts) = dict.get("Font") {
        let font_dict = match fonts {
            PdfValue::Object(PdfObject::Dictionary(d)) => Some(d),
            PdfValue::Reference(id, _) => object_dict(doc, *id),
            _ => None,
        };
        if let Some(font_dict) = font_dict {
            for (name, font_ref) in font_dict {
                let Some(font_id) = as_ref_id(font_ref) else {
                    continue;
                };
                let Some(font_obj) = object_dict(doc, font_id) else {
                    continue;
                };
                let metrics = font_metrics_for(doc, font_obj);
                out.insert(name.clone(), metrics);
            }
        }
    }
}

fn font_metrics_for(doc: &PdfDocument, font: &HashMap<String, PdfValue>) -> FontMetrics {
    let base_font = match font.get("BaseFont") {
        Some(v) => match v {
            PdfValue::Object(PdfObject::Name(n)) => Some(n.clone()),
            PdfValue::Object(PdfObject::String(s)) => Some(s.trim_start_matches('/').to_string()),
            _ => None,
        },
        None => None,
    };
    let is_base14 = base_font.as_deref().map(is_base14_font).unwrap_or(false);
    if is_base14 && let Some(name) = base_font {
        return base14_font_metrics(&name);
    }
    // Widths[] for simple Type1 fonts
    if let (Some(fc), Some(lc), Some(widths)) = (
        font.get("FirstChar")
            .and_then(|v| as_number(doc, v))
            .map(|n| n as u32),
        font.get("LastChar")
            .and_then(|v| as_number(doc, v).map(|n| n as u32)),
        font.get("Widths").and_then(|v| as_array(doc, v)),
    ) {
        let mut map = HashMap::new();
        for (i, item) in widths.iter().enumerate() {
            if let Some(w) = as_number(doc, item) {
                let ch = fc + i as u32;
                if ch <= lc {
                    map.insert(ch, (w as u16).max(1));
                }
            }
        }
        return FontMetrics {
            widths: map,
            default_width: 500,
        };
    }
    // CIDFont /W array
    if let Some(w_array) = font.get("W").and_then(|v| as_array(doc, v)) {
        let mut map = HashMap::new();
        let mut i = 0;
        while i < w_array.len() {
            if i + 2 < w_array.len()
                && let (Some(c_first), Some(c_last)) = (
                    as_number(doc, &w_array[i]).map(|n| n as u32),
                    as_number(doc, &w_array[i + 1]).map(|n| n as u32),
                )
            {
                let width = as_number(doc, &w_array[i + 2]).unwrap_or(500.0) as u16;
                for c in c_first..=c_last {
                    map.insert(c, width);
                }
                i += 3;
                continue;
            }
            if i + 1 < w_array.len()
                && let Some(c_first) = as_number(doc, &w_array[i]).map(|n| n as u32)
            {
                let width = as_number(doc, &w_array[i + 1]).unwrap_or(500.0) as u16;
                map.insert(c_first, width);
                i += 2;
                continue;
            }
            break;
        }
        return FontMetrics {
            widths: map,
            default_width: 500,
        };
    }
    FontMetrics {
        widths: HashMap::new(),
        default_width: 500,
    }
}

fn as_number(doc: &PdfDocument, val: &PdfValue) -> Option<f32> {
    match val {
        PdfValue::Object(PdfObject::Number(n)) => Some(*n as f32),
        PdfValue::Reference(id, _) => doc.objects.get(id).and_then(|o| {
            if let PdfObject::Number(n) = o {
                Some(*n as f32)
            } else {
                None
            }
        }),
        PdfValue::Object(PdfObject::String(s)) => {
            let trimmed = s.trim().trim_matches(|c| c == '[' || c == ']');
            trimmed
                .split_whitespace()
                .next()
                .and_then(|t| t.parse::<f32>().ok())
        }
        _ => None,
    }
}

fn is_base14_font(name: &str) -> bool {
    matches!(
        name,
        "Helvetica"
            | "Helvetica-Bold"
            | "Helvetica-Oblique"
            | "Helvetica-BoldOblique"
            | "Times-Roman"
            | "Times-Bold"
            | "Times-Italic"
            | "Times-BoldItalic"
            | "Courier"
            | "Courier-Bold"
            | "Courier-Oblique"
            | "Courier-BoldOblique"
            | "Symbol"
            | "ZapfDingbats"
    )
}

fn base14_font_metrics(name: &str) -> FontMetrics {
    let widths = match name {
        "Helvetica" | "Helvetica-Bold" | "Helvetica-Oblique" | "Helvetica-BoldOblique" => {
            helvetica_widths()
        }
        "Times-Roman" | "Times-Bold" | "Times-Italic" | "Times-BoldItalic" => times_widths(),
        "Courier" | "Courier-Bold" | "Courier-Oblique" | "Courier-BoldOblique" => courier_widths(),
        _ => HashMap::new(),
    };
    FontMetrics {
        widths,
        default_width: 500,
    }
}

fn helvetica_widths() -> HashMap<u32, u16> {
    let raw: &[(u32, u16)] = &[
        (32, 278),
        (33, 278),
        (34, 355),
        (35, 556),
        (36, 556),
        (37, 889),
        (38, 667),
        (39, 191),
        (40, 333),
        (41, 333),
        (42, 389),
        (43, 584),
        (44, 278),
        (45, 333),
        (46, 278),
        (47, 278),
        (48, 556),
        (49, 556),
        (50, 556),
        (51, 556),
        (52, 556),
        (53, 556),
        (54, 556),
        (55, 556),
        (56, 556),
        (57, 556),
        (58, 278),
        (59, 278),
        (60, 584),
        (61, 584),
        (62, 584),
        (63, 556),
        (64, 1015),
        (65, 667),
        (66, 667),
        (67, 722),
        (68, 722),
        (69, 667),
        (70, 611),
        (71, 778),
        (72, 722),
        (73, 278),
        (74, 500),
        (75, 667),
        (76, 556),
        (77, 833),
        (78, 722),
        (79, 778),
        (80, 667),
        (81, 778),
        (82, 722),
        (83, 667),
        (84, 611),
        (85, 722),
        (86, 667),
        (87, 944),
        (88, 667),
        (89, 667),
        (90, 611),
        (97, 556),
        (98, 556),
        (99, 500),
        (100, 556),
        (101, 556),
        (102, 278),
        (103, 556),
        (104, 556),
        (105, 222),
        (106, 222),
        (107, 500),
        (108, 222),
        (109, 833),
        (110, 556),
        (111, 556),
        (112, 556),
        (113, 556),
        (114, 333),
        (115, 500),
        (116, 278),
        (117, 556),
        (118, 500),
        (119, 722),
        (120, 500),
        (121, 500),
        (122, 500),
    ];
    raw.iter().copied().collect()
}

fn times_widths() -> HashMap<u32, u16> {
    let raw: &[(u32, u16)] = &[
        (32, 250),
        (33, 333),
        (34, 408),
        (35, 500),
        (36, 500),
        (37, 833),
        (38, 778),
        (39, 180),
        (40, 333),
        (41, 333),
        (42, 500),
        (43, 564),
        (44, 250),
        (45, 333),
        (46, 250),
        (47, 278),
        (48, 500),
        (49, 500),
        (50, 500),
        (51, 500),
        (52, 500),
        (53, 500),
        (54, 500),
        (55, 500),
        (56, 500),
        (57, 500),
        (58, 278),
        (59, 278),
        (60, 564),
        (61, 564),
        (62, 564),
        (63, 444),
        (64, 921),
        (65, 722),
        (66, 667),
        (67, 667),
        (68, 722),
        (69, 611),
        (70, 556),
        (71, 722),
        (72, 722),
        (73, 333),
        (74, 389),
        (75, 722),
        (76, 611),
        (77, 889),
        (78, 722),
        (79, 722),
        (80, 556),
        (81, 722),
        (82, 667),
        (83, 556),
        (84, 611),
        (85, 722),
        (86, 722),
        (87, 944),
        (88, 722),
        (89, 722),
        (90, 611),
        (97, 444),
        (98, 500),
        (99, 444),
        (100, 500),
        (101, 444),
        (102, 333),
        (103, 500),
        (104, 500),
        (105, 278),
        (106, 278),
        (107, 500),
        (108, 278),
        (109, 778),
        (110, 500),
        (111, 500),
        (112, 500),
        (113, 500),
        (114, 333),
        (115, 389),
        (116, 278),
        (117, 500),
        (118, 500),
        (119, 722),
        (120, 500),
        (121, 500),
        (122, 444),
    ];
    raw.iter().copied().collect()
}

fn courier_widths() -> HashMap<u32, u16> {
    let mut out = HashMap::new();
    for c in 32..=127 {
        out.insert(c, 600);
    }
    out
}
