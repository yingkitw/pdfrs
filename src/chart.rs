//! Simple chart parsing and layout helpers for Markdown → PDF.
//!
//! Fenced Markdown blocks use language tags such as `chart`, `chart-bar`,
//! `chart-line`, or `chart-pie`, with lines like `title: …` and `Label, value`.
//!
//! Supported kinds: bar (default), line, pie.

use crate::elements::{ChartKind, Element};

/// One labeled numeric value in a chart.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartPoint {
    pub label: String,
    pub value: f32,
}

/// Parsed chart ready for rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub title: Option<String>,
    pub points: Vec<ChartPoint>,
}

/// Palette used for series fills/strokes (RGB 0..=1).
pub const CHART_COLORS: [(f32, f32, f32); 8] = [
    (0.20, 0.45, 0.75),
    (0.85, 0.40, 0.25),
    (0.30, 0.65, 0.40),
    (0.70, 0.45, 0.75),
    (0.90, 0.70, 0.20),
    (0.25, 0.65, 0.70),
    (0.55, 0.55, 0.55),
    (0.75, 0.35, 0.50),
];

/// If `language` is a chart fence (`chart`, `chart-bar`, …), parse `code` into a chart element.
pub fn try_parse_chart_element(language: &str, code: &str) -> Option<Element> {
    let kind = parse_chart_language(language)?;
    let spec = parse_chart_body(kind, code);
    if spec.points.is_empty() {
        return None;
    }
    Some(Element::Chart {
        kind: spec.kind,
        title: spec.title,
        points: spec
            .points
            .into_iter()
            .map(|p| (p.label, p.value))
            .collect(),
    })
}

fn parse_chart_language(language: &str) -> Option<ChartKind> {
    let lang = language.trim().to_ascii_lowercase();
    if lang.is_empty() {
        return None;
    }
    let rest = if lang == "chart" {
        ""
    } else if let Some(r) = lang.strip_prefix("chart-") {
        r
    } else if let Some(r) = lang.strip_prefix("chart:") {
        r
    } else if let Some(r) = lang.strip_prefix("chart ") {
        r.trim()
    } else {
        return None;
    };
    Some(match rest {
        "" | "bar" | "column" => ChartKind::Bar,
        "line" => ChartKind::Line,
        "pie" => ChartKind::Pie,
        _ => ChartKind::Bar,
    })
}

fn parse_chart_body(kind: ChartKind, code: &str) -> ChartSpec {
    let mut title = None;
    let mut points = Vec::new();
    for raw in code.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(t) = line
            .strip_prefix("title:")
            .or_else(|| line.strip_prefix("Title:"))
        {
            let t = t.trim();
            if !t.is_empty() {
                title = Some(t.to_string());
            }
            continue;
        }
        // "Label, value" or "Label: value"
        let (label, value_str) = if let Some((a, b)) = line.split_once(',') {
            (a.trim(), b.trim())
        } else if let Some((a, b)) = line.split_once(':') {
            (a.trim(), b.trim())
        } else {
            continue;
        };
        if label.is_empty() {
            continue;
        }
        if let Ok(value) = value_str.replace(',', "").parse::<f32>() {
            points.push(ChartPoint {
                label: label.to_string(),
                value,
            });
        }
    }
    ChartSpec { kind, title, points }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bar_chart_block() {
        let el = try_parse_chart_element(
            "chart bar",
            "title: Sales\nQ1, 10\nQ2, 20.5\n# comment\nQ3: 15\n",
        )
        .expect("chart");
        match el {
            Element::Chart {
                kind,
                title,
                points,
            } => {
                assert_eq!(kind, ChartKind::Bar);
                assert_eq!(title.as_deref(), Some("Sales"));
                assert_eq!(points.len(), 3);
                assert_eq!(points[1], ("Q2".into(), 20.5));
            }
            _ => panic!("expected Chart"),
        }
    }

    #[test]
    fn reject_non_chart_language() {
        assert!(try_parse_chart_element("rust", "fn main() {}").is_none());
    }
}
