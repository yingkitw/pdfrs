//! Simple chart parsing and layout helpers for Markdown → PDF.
//!
//! Fenced Markdown blocks use language tags such as `chart`, `chart-bar`,
//! `chart-line`, or `chart-pie`, with lines like `title: …` and `Label, value`.
//!
//! Supported kinds: bar (default), line, pie.

use crate::elements::{ChartKind, ChartSeries, Element};

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
    pub series: Vec<ChartSeries>,
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
    if spec.points.is_empty() && spec.series.is_empty() {
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
        series: spec.series,
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
        "stacked-bar" | "stackedbar" | "stacked" => ChartKind::StackedBar,
        _ => ChartKind::Bar,
    })
}

fn parse_chart_body(kind: ChartKind, code: &str) -> ChartSpec {
    let mut title = None;
    let mut points = Vec::new();
    let mut series_names: Vec<String> = Vec::new();
    let mut series_data: Vec<Vec<f32>> = Vec::new();

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
        // "series:" directive declares series names for multi-series charts
        if let Some(s) = line
            .strip_prefix("series:")
            .or_else(|| line.strip_prefix("Series:"))
        {
            series_names = s
                .split(',')
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .collect();
            series_data.resize(series_names.len(), Vec::new());
            continue;
        }
        // Parse data line: "Label, v1, v2, v3, ..."
        let parts: Vec<&str> = line.split(',').map(|p| p.trim()).collect();
        if parts.len() < 2 {
            if let Some((a, b)) = line.split_once(':') {
                let (label, value_str) = (a.trim(), b.trim());
                if !label.is_empty()
                    && let Ok(value) = value_str.replace(',', "").parse::<f32>() {
                        points.push(ChartPoint {
                            label: label.to_string(),
                            value,
                        });
                    }
            }
            continue;
        }
        let label = parts[0].to_string();
        if label.is_empty() {
            continue;
        }
        // If we have series names, parse multi-column data
        if !series_names.is_empty() {
            let values: Vec<f32> = parts[1..]
                .iter()
                .filter_map(|s| s.parse::<f32>().ok())
                .collect();
            if values.len() == series_names.len() {
                for (i, v) in values.iter().enumerate() {
                    series_data[i].push(*v);
                }
                // Use first series value as the "point" for label/axis purposes
                points.push(ChartPoint {
                    label: label.clone(),
                    value: values.iter().sum(),
                });
            }
        } else {
            // Single-series: "Label, value"
            if let Ok(value) = parts[1].parse::<f32>() {
                points.push(ChartPoint {
                    label,
                    value,
                });
            }
        }
    }

    let series = if !series_names.is_empty() {
        series_names
            .into_iter()
            .zip(series_data)
            .map(|(name, values)| ChartSeries { name, values })
            .collect()
    } else {
        Vec::new()
    };

    ChartSpec {
        kind,
        title,
        points,
        series,
    }
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
                series,
            } => {
                assert_eq!(kind, ChartKind::Bar);
                assert_eq!(title.as_deref(), Some("Sales"));
                assert_eq!(points.len(), 3);
                assert_eq!(points[1], ("Q2".into(), 20.5));
                assert!(series.is_empty());
            }
            _ => panic!("expected Chart"),
        }
    }

    #[test]
    fn parse_stacked_bar_chart() {
        let el = try_parse_chart_element(
            "chart-stacked-bar",
            "title: Revenue\nseries: Product A, Product B, Product C\nQ1, 10, 20, 5\nQ2, 15, 25, 10\nQ3, 20, 30, 15\n",
        )
        .expect("chart");
        match el {
            Element::Chart {
                kind,
                title,
                points,
                series,
            } => {
                assert_eq!(kind, ChartKind::StackedBar);
                assert_eq!(title.as_deref(), Some("Revenue"));
                assert_eq!(points.len(), 3);
                assert_eq!(points[0].0, "Q1");
                assert_eq!(points[0].1, 35.0); // 10+20+5
                assert_eq!(series.len(), 3);
                assert_eq!(series[0].name, "Product A");
                assert_eq!(series[0].values, vec![10.0, 15.0, 20.0]);
                assert_eq!(series[1].name, "Product B");
                assert_eq!(series[1].values, vec![20.0, 25.0, 30.0]);
                assert_eq!(series[2].name, "Product C");
                assert_eq!(series[2].values, vec![5.0, 10.0, 15.0]);
            }
            _ => panic!("expected Chart"),
        }
    }

    #[test]
    fn reject_non_chart_language() {
        assert!(try_parse_chart_element("rust", "fn main() {}").is_none());
    }
}
