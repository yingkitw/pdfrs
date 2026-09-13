//! Chart rendering: bar, line, pie, and stacked-bar figures with legends.

use crate::elements::ChartKind;

use super::{ContentStreamBuilder, FONT_HELVETICA};
use crate::pdf_generator::layout::{Color, TextAlign, line_height};

impl ContentStreamBuilder {
    /// Draw a bar / line / pie / stacked-bar chart from labeled numeric points.
    pub(super) fn emit_chart(
        &mut self,
        kind: ChartKind,
        title: &Option<String>,
        points: &[(String, f32)],
        series: &[crate::elements::ChartSeries],
    ) {
        if points.is_empty() && series.is_empty() {
            return;
        }

        let title_h = if title.is_some() {
            line_height(self.base_font_size)
        } else {
            0.0
        };
        let plot_h = match kind {
            ChartKind::Pie => 170.0,
            _ => 150.0,
        };
        let legend_h = if matches!(kind, ChartKind::Pie) {
            (points.len() as f32) * line_height(self.base_font_size * 0.8)
        } else if !series.is_empty() {
            (series.len() as f32) * line_height(self.base_font_size * 0.8)
        } else {
            0.0
        };
        let total_h = title_h + plot_h + legend_h + 16.0;
        self.ensure_space(total_h);

        self.figure_counter += 1;
        let fig_title = match title {
            Some(t) => format!("Figure {}. {}", self.figure_counter, t),
            None => format!("Figure {}.", self.figure_counter),
        };
        self.set_font_with_style(self.base_font_size, true, false);
        self.emit_line_aligned(&fig_title, self.base_font_size, TextAlign::Center);
        self.set_font_with_style(self.base_font_size, false, false);

        let left = self.content_left();
        let width = self.content_width();
        let top = self.y;
        let bottom = top - plot_h;

        self.current.extend_from_slice(b"ET\n");

        match kind {
            ChartKind::Bar => self.draw_bar_chart(left, bottom, width, plot_h, points),
            ChartKind::Line => self.draw_line_chart(left, bottom, width, plot_h, points),
            ChartKind::Pie => self.draw_pie_chart(left, bottom, width, plot_h, points),
            ChartKind::StackedBar => {
                self.draw_stacked_bar_chart(left, bottom, width, plot_h, points, series)
            }
        }

        self.current.extend_from_slice(b"BT\n");
        self.set_font_with_style(self.base_font_size, false, false);
        self.reset_color();
        self.y = bottom - 8.0;
        self.mark_content_placed();

        if matches!(kind, ChartKind::Pie) {
            let label_size = self.base_font_size * 0.8;
            for (i, (label, value)) in points.iter().enumerate() {
                let (r, g, b) = crate::chart::CHART_COLORS[i % crate::chart::CHART_COLORS.len()];
                self.set_color(Color::rgb(r, g, b));
                self.emit_line(
                    &format!("• {} ({})", label, format_chart_value(*value)),
                    label_size,
                );
            }
            self.reset_color();
        } else if !series.is_empty() {
            // Legend for multi-series charts
            let label_size = self.base_font_size * 0.8;
            for (i, s) in series.iter().enumerate() {
                let (r, g, b) = crate::chart::CHART_COLORS[i % crate::chart::CHART_COLORS.len()];
                self.set_color(Color::rgb(r, g, b));
                self.emit_line(&format!("■ {}", s.name), label_size);
            }
            self.reset_color();
        }

        self.emit_empty_line();
    }

    fn draw_bar_chart(
        &mut self,
        left: f32,
        bottom: f32,
        width: f32,
        height: f32,
        points: &[(String, f32)],
    ) {
        let max_v = points
            .iter()
            .map(|(_, v)| v.abs())
            .fold(0.0_f32, f32::max)
            .max(1.0);
        let axis = Color::rgb(0.35, 0.35, 0.35);
        let pad_l = 28.0;
        let pad_b = 22.0;
        let pad_t = 8.0;
        let plot_x = left + pad_l;
        let plot_w = width - pad_l - 4.0;
        let plot_y0 = bottom + pad_b;
        let plot_h = height - pad_b - pad_t;

        // Axes
        self.append_stroke(axis, 0.8);
        self.append_line(plot_x, plot_y0, plot_x + plot_w, plot_y0);
        self.append_line(plot_x, plot_y0, plot_x, plot_y0 + plot_h);

        let n = points.len() as f32;
        let slot = plot_w / n;
        let bar_w = (slot * 0.62).max(4.0);

        for (i, (label, value)) in points.iter().enumerate() {
            let (r, g, b) = crate::chart::CHART_COLORS[i % crate::chart::CHART_COLORS.len()];
            let h = (value.abs() / max_v) * plot_h;
            let x = plot_x + i as f32 * slot + (slot - bar_w) / 2.0;
            let y = plot_y0;
            self.current.extend_from_slice(
                format!(
                    "{} {} {} rg\n{:.2} {:.2} {:.2} {:.2} re f\n",
                    r, g, b, x, y, bar_w, h
                )
                .as_bytes(),
            );
            // Label under bar (short)
            let short = truncate_label(label, 8);
            self.append_fill_text(
                &short,
                x + bar_w / 2.0 - self.estimate_text_width(&short, 7.0) / 2.0,
                bottom + 6.0,
                7.0,
                axis,
            );
        }
    }

    fn draw_line_chart(
        &mut self,
        left: f32,
        bottom: f32,
        width: f32,
        height: f32,
        points: &[(String, f32)],
    ) {
        let max_v = points
            .iter()
            .map(|(_, v)| *v)
            .fold(f32::NEG_INFINITY, f32::max);
        let min_v = points.iter().map(|(_, v)| *v).fold(f32::INFINITY, f32::min);
        let span = (max_v - min_v).abs().max(1.0);
        let axis = Color::rgb(0.35, 0.35, 0.35);
        let pad_l = 28.0;
        let pad_b = 22.0;
        let pad_t = 8.0;
        let plot_x = left + pad_l;
        let plot_w = width - pad_l - 4.0;
        let plot_y0 = bottom + pad_b;
        let plot_h = height - pad_b - pad_t;

        self.append_stroke(axis, 0.8);
        self.append_line(plot_x, plot_y0, plot_x + plot_w, plot_y0);
        self.append_line(plot_x, plot_y0, plot_x, plot_y0 + plot_h);

        let n = points.len().max(1);
        let mut coords = Vec::with_capacity(n);
        for (i, (_, value)) in points.iter().enumerate() {
            let t = if n == 1 {
                0.5
            } else {
                i as f32 / (n - 1) as f32
            };
            let x = plot_x + t * plot_w;
            let y = plot_y0 + ((*value - min_v) / span) * plot_h;
            coords.push((x, y));
        }

        let (r, g, b) = crate::chart::CHART_COLORS[0];
        self.current
            .extend_from_slice(format!("{} {} {} RG\n1.5 w\n", r, g, b).as_bytes());
        if let Some((x0, y0)) = coords.first() {
            self.current
                .extend_from_slice(format!("{:.2} {:.2} m\n", x0, y0).as_bytes());
            for (x, y) in coords.iter().skip(1) {
                self.current
                    .extend_from_slice(format!("{:.2} {:.2} l\n", x, y).as_bytes());
            }
            self.current.extend_from_slice(b"S\n");
        }
        for (x, y) in &coords {
            // Small filled square as a point marker (PDF has no `arc` operator).
            self.current.extend_from_slice(
                format!(
                    "{} {} {} rg\n{:.2} {:.2} 3.5 3.5 re f\n",
                    r,
                    g,
                    b,
                    x - 1.75,
                    y - 1.75
                )
                .as_bytes(),
            );
        }

        for (i, (label, _)) in points.iter().enumerate() {
            let short = truncate_label(label, 8);
            let (x, _) = coords[i];
            self.append_fill_text(
                &short,
                x - self.estimate_text_width(&short, 7.0) / 2.0,
                bottom + 6.0,
                7.0,
                axis,
            );
        }
    }

    fn draw_pie_chart(
        &mut self,
        left: f32,
        bottom: f32,
        width: f32,
        height: f32,
        points: &[(String, f32)],
    ) {
        let total: f32 = points.iter().map(|(_, v)| v.abs()).sum::<f32>().max(1.0);
        let cx = left + width * 0.42;
        let cy = bottom + height * 0.52;
        let radius = (width.min(height) * 0.32).min(70.0);

        let mut angle = 0.0_f32; // radians from +x
        for (i, (_, value)) in points.iter().enumerate() {
            let sweep = (value.abs() / total) * std::f32::consts::TAU;
            if sweep <= 0.0 {
                continue;
            }
            let (r, g, b) = crate::chart::CHART_COLORS[i % crate::chart::CHART_COLORS.len()];
            self.append_pie_slice(cx, cy, radius, angle, angle + sweep, Color::rgb(r, g, b));
            angle += sweep;
        }
    }

    fn draw_stacked_bar_chart(
        &mut self,
        left: f32,
        bottom: f32,
        width: f32,
        height: f32,
        points: &[(String, f32)],
        series: &[crate::elements::ChartSeries],
    ) {
        if series.is_empty() || points.is_empty() {
            // Fall back to simple bar chart if no series data
            self.draw_bar_chart(left, bottom, width, height, points);
            return;
        }

        // Find max stacked total across all categories
        let max_v = points
            .iter()
            .map(|(_, v)| v.abs())
            .fold(0.0_f32, f32::max)
            .max(1.0);
        let axis = Color::rgb(0.35, 0.35, 0.35);
        let pad_l = 28.0;
        let pad_b = 22.0;
        let pad_t = 8.0;
        let plot_x = left + pad_l;
        let plot_w = width - pad_l - 4.0;
        let plot_y0 = bottom + pad_b;
        let plot_h = height - pad_b - pad_t;

        // Axes
        self.append_stroke(axis, 0.8);
        self.append_line(plot_x, plot_y0, plot_x + plot_w, plot_y0);
        self.append_line(plot_x, plot_y0, plot_x, plot_y0 + plot_h);

        let n = points.len() as f32;
        let slot = plot_w / n;
        let bar_w = (slot * 0.62).max(4.0);

        for (cat_i, (label, _)) in points.iter().enumerate() {
            let x = plot_x + cat_i as f32 * slot + (slot - bar_w) / 2.0;
            let mut stack_y = plot_y0;

            for (s_i, s) in series.iter().enumerate() {
                if cat_i >= s.values.len() {
                    break;
                }
                let value = s.values[cat_i];
                let h = (value.abs() / max_v) * plot_h;
                let (r, g, b) = crate::chart::CHART_COLORS[s_i % crate::chart::CHART_COLORS.len()];
                self.current.extend_from_slice(
                    format!(
                        "{} {} {} rg\n{:.2} {:.2} {:.2} {:.2} re f\n",
                        r, g, b, x, stack_y, bar_w, h
                    )
                    .as_bytes(),
                );
                stack_y += h;
            }

            // Label under bar
            let short = truncate_label(label, 8);
            self.append_fill_text(
                &short,
                x + bar_w / 2.0 - self.estimate_text_width(&short, 7.0) / 2.0,
                bottom + 6.0,
                7.0,
                axis,
            );
        }
    }

    fn append_pie_slice(&mut self, cx: f32, cy: f32, radius: f32, a0: f32, a1: f32, fill: Color) {
        // Approximate arc with line segments.
        let steps = ((a1 - a0).abs() / 0.2).ceil().max(2.0) as usize;
        self.current.extend_from_slice(
            format!(
                "{} {} {} rg\n{:.2} {:.2} m\n",
                fill.r, fill.g, fill.b, cx, cy
            )
            .as_bytes(),
        );
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let a = a0 + (a1 - a0) * t;
            let x = cx + radius * a.cos();
            let y = cy + radius * a.sin();
            self.current
                .extend_from_slice(format!("{:.2} {:.2} l\n", x, y).as_bytes());
        }
        self.current.extend_from_slice(b"h f\n");
    }

    fn append_stroke(&mut self, color: Color, width: f32) {
        self.current.extend_from_slice(
            format!("{} {} {} RG\n{} w\n", color.r, color.g, color.b, width).as_bytes(),
        );
    }

    fn append_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.current.extend_from_slice(
            format!("{:.2} {:.2} m {:.2} {:.2} l S\n", x1, y1, x2, y2).as_bytes(),
        );
    }

    fn append_fill_text(&mut self, text: &str, x: f32, y: f32, size: f32, color: Color) {
        // Nested BT inside graphics section (we are outside the main BT).
        self.current.extend_from_slice(b"BT\n");
        self.current.extend_from_slice(
            format!(
                "/{} {} Tf\n{} {} {} rg\n1 0 0 1 {:.2} {:.2} Tm\n{} Tj\nET\n",
                FONT_HELVETICA,
                size,
                color.r,
                color.g,
                color.b,
                x,
                y,
                self.encode_text_for_current_font(text)
            )
            .as_bytes(),
        );
    }
}

fn truncate_label(label: &str, max_chars: usize) -> String {
    let count = label.chars().count();
    if count <= max_chars {
        return label.to_string();
    }
    label
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn format_chart_value(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i64)
    } else {
        format!("{:.1}", v)
    }
}
