//! Table rendering module for PDF generation
//!
//! This module provides a trait-based, modular approach to rendering tables in PDFs.
//! It follows the Strategy pattern for different table rendering approaches.

use crate::elements::TableAlignment;
use anyhow::Result;

/// Configuration for table styling
#[derive(Debug, Clone)]
pub struct TableStyle {
    /// Padding inside each cell (in points)
    pub cell_padding: f32,
    /// Margin above the table (in points)
    pub margin_top: f32,
    /// Margin below the table (in points)
    pub margin_bottom: f32,
    /// Outer border width (in points)
    pub border_width: f32,
    /// Inner grid line width (in points)
    pub grid_line_width: f32,
    /// Outer border color (RGB 0-1)
    pub border_color: (f32, f32, f32),
    /// Inner grid line color (RGB 0-1)
    pub grid_color: (f32, f32, f32),
}

impl Default for TableStyle {
    fn default() -> Self {
        Self {
            cell_padding: 8.0,
            margin_top: 16.0,
            margin_bottom: 16.0,
            border_width: 1.5,
            grid_line_width: 0.75,
            border_color: (0.0, 0.0, 0.0),
            grid_color: (0.75, 0.75, 0.75),
        }
    }
}

/// Represents a single table cell with its content and alignment
#[derive(Debug, Clone)]
pub struct TableCell {
    pub content: String,
    pub alignment: TableAlignment,
}

impl TableCell {
    pub fn new(content: String, alignment: TableAlignment) -> Self {
        Self { content, alignment }
    }

    pub fn left(content: &str) -> Self {
        Self::new(content.to_string(), TableAlignment::Left)
    }

    pub fn center(content: &str) -> Self {
        Self::new(content.to_string(), TableAlignment::Center)
    }

    pub fn right(content: &str) -> Self {
        Self::new(content.to_string(), TableAlignment::Right)
    }
}

/// Represents a table row containing multiple cells
#[derive(Debug, Clone)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}

impl TableRow {
    pub fn new(cells: Vec<TableCell>) -> Self {
        Self { cells }
    }

    /// Create a row from strings with default left alignment
    pub fn from_strings(cells: &[&str]) -> Self {
        Self {
            cells: cells.iter().map(|s| TableCell::left(s)).collect()
        }
    }
}

/// Measured table dimensions for layout
#[derive(Debug, Clone)]
pub struct TableDimensions {
    pub column_widths: Vec<f32>,
    pub row_heights: Vec<f32>,
    pub total_width: f32,
    pub total_height: f32,
    pub num_cols: usize,
    pub num_rows: usize,
}

/// Line wrapping result for a cell
#[derive(Debug, Clone)]
pub struct WrappedLines {
    pub lines: Vec<String>,
    pub line_count: usize,
}

impl WrappedLines {
    pub fn new(lines: Vec<String>) -> Self {
        let line_count = lines.len();
        Self { lines, line_count }
    }

    pub fn empty() -> Self {
        Self::new(vec![String::new()])
    }
}

/// Trait for table rendering strategies
///
/// This allows different table rendering implementations to be plugged in.
pub trait TableRenderer {
    /// Calculate the dimensions of a table before rendering
    fn calculate_dimensions(
        &self,
        rows: &[TableRow],
        style: &TableStyle,
        base_font_size: f32,
        max_width: f32,
    ) -> TableDimensions;

    /// Wrap text into lines based on available width
    fn wrap_text(&self, text: &str, max_chars: usize) -> WrappedLines;

    /// Calculate the X position for text based on alignment
    fn calculate_text_x(
        &self,
        alignment: &TableAlignment,
        cell_x: f32,
        cell_width: f32,
        text_width: f32,
        padding: f32,
    ) -> f32;
}

/// Default implementation of table rendering
pub struct DefaultTableRenderer;

impl TableRenderer for DefaultTableRenderer {
    fn calculate_dimensions(
        &self,
        rows: &[TableRow],
        style: &TableStyle,
        base_font_size: f32,
        max_width: f32,
    ) -> TableDimensions {
        if rows.is_empty() {
            return TableDimensions {
                column_widths: vec![],
                row_heights: vec![],
                total_width: 0.0,
                total_height: 0.0,
                num_cols: 0,
                num_rows: 0,
            };
        }

        let num_cols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
        let num_rows = rows.len();
        let approx_char_width = base_font_size * 0.5;
        let line_h = base_font_size * 1.4;

        // Calculate column widths
        let mut col_widths: Vec<f32> = vec![0.0; num_cols];
        for row in rows {
            for (col_idx, cell) in row.cells.iter().enumerate() {
                if col_idx < num_cols {
                    let cell_width = cell.content.len() as f32 * approx_char_width + style.cell_padding * 2.0;
                    col_widths[col_idx] = col_widths[col_idx].max(cell_width);
                }
            }
        }

        // Scale to fit max width
        let total_width: f32 = col_widths.iter().sum();
        if total_width > max_width {
            let scale = max_width / total_width;
            for width in &mut col_widths {
                *width *= scale;
            }
        }

        // Calculate row heights
        let mut row_heights: Vec<f32> = vec![0.0; num_rows];
        for (row_idx, row) in rows.iter().enumerate() {
            let mut max_lines = 1;
            for (col_idx, cell) in row.cells.iter().enumerate() {
                if col_idx >= num_cols { break; }
                let max_chars = ((col_widths[col_idx] - style.cell_padding * 2.0) / approx_char_width).floor().max(1.0) as usize;
                let wrapped = self.wrap_text(&cell.content, max_chars);
                max_lines = max_lines.max(wrapped.line_count);
            }
            row_heights[row_idx] = max_lines as f32 * line_h + style.cell_padding * 2.0;
        }

        let total_width: f32 = col_widths.iter().sum();
        let total_height: f32 = row_heights.iter().sum();

        TableDimensions {
            column_widths: col_widths,
            row_heights,
            total_width,
            total_height,
            num_cols,
            num_rows,
        }
    }

    fn wrap_text(&self, text: &str, max_chars: usize) -> WrappedLines {
        if text.len() <= max_chars {
            return WrappedLines::new(vec![text.to_string()]);
        }

        let words: Vec<&str> = text.split_whitespace().collect();
        let mut lines = Vec::new();
        let mut current_line = String::new();
        let mut current_len = 0;

        for word in words {
            let new_len = if current_len == 0 {
                word.len()
            } else {
                current_len + 1 + word.len()
            };

            if new_len <= max_chars {
                if current_len == 0 {
                    current_line = word.to_string();
                    current_len = word.len();
                } else {
                    current_line.push(' ');
                    current_line.push_str(word);
                    current_len = new_len;
                }
            } else {
                if !current_line.is_empty() {
                    lines.push(current_line);
                }
                current_line = word.to_string();
                current_len = word.len();
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }

        if lines.is_empty() {
            lines.push(String::new())
        }

        WrappedLines::new(lines)
    }

    fn calculate_text_x(
        &self,
        alignment: &TableAlignment,
        cell_x: f32,
        cell_width: f32,
        text_width: f32,
        padding: f32,
    ) -> f32 {
        match alignment {
            TableAlignment::Left => cell_x + padding,
            TableAlignment::Center => cell_x + (cell_width - text_width) / 2.0,
            TableAlignment::Right => cell_x + cell_width - padding - text_width,
        }
    }
}

impl Default for DefaultTableRenderer {
    fn default() -> Self {
        Self
    }
}

/// Helper functions for PDF table rendering
pub struct PdfTableHelper {
    renderer: Box<dyn TableRenderer>,
    style: TableStyle,
}

impl PdfTableHelper {
    pub fn new(renderer: Box<dyn TableRenderer>) -> Self {
        Self {
            renderer,
            style: TableStyle::default(),
        }
    }

    pub fn with_style(mut self, style: TableStyle) -> Self {
        self.style = style;
        self
    }

    pub fn style(&self) -> &TableStyle {
        &self.style
    }

    pub fn renderer(&self) -> &dyn TableRenderer {
        self.renderer.as_ref()
    }

    /// Convert string rows to TableCell rows with alignments
    pub fn convert_rows(&self, rows: &[Vec<String>], alignments: Option<&[TableAlignment]>) -> Vec<TableRow> {
        rows.iter().enumerate().map(|(row_idx, row)| {
            let cells: Vec<TableCell> = row.iter().enumerate().map(|(col_idx, cell)| {
                let alignment = alignments
                    .and_then(|a| a.get(col_idx))
                    .copied()
                    .unwrap_or(TableAlignment::Left);
                TableCell::new(cell.clone(), alignment)
            }).collect();
            TableRow { cells }
        }).collect()
    }

    /// Escape special PDF string characters (public static helper)
    pub fn escape_pdf_string_static(text: &str) -> String {
        text.replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
    }

    /// Escape special PDF string characters (instance method)
    pub fn escape_pdf_string(&self, text: &str) -> String {
        Self::escape_pdf_string_static(text)
    }
}

impl Default for PdfTableHelper {
    fn default() -> Self {
        Self::new(Box::new(DefaultTableRenderer::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_cell_creation() {
        let cell = TableCell::left("test");
        assert_eq!(cell.content, "test");
        assert!(matches!(cell.alignment, TableAlignment::Left));
    }

    #[test]
    fn test_table_row_from_strings() {
        let row = TableRow::from_strings(&["A", "B", "C"]);
        assert_eq!(row.cells.len(), 3);
        assert_eq!(row.cells[0].content, "A");
    }

    #[test]
    fn test_text_wrapping() {
        let renderer = DefaultTableRenderer;
        let wrapped = renderer.wrap_text("hello world test", 10);
        assert!(wrapped.line_count > 1);
    }

    #[test]
    fn test_text_wrapping_single_word() {
        let renderer = DefaultTableRenderer;
        let wrapped = renderer.wrap_text("hello", 10);
        assert_eq!(wrapped.line_count, 1);
        assert_eq!(wrapped.lines[0], "hello");
    }

    #[test]
    fn test_calculate_text_x_left() {
        let renderer = DefaultTableRenderer;
        let x = renderer.calculate_text_x(&TableAlignment::Left, 100.0, 50.0, 20.0, 10.0);
        assert_eq!(x, 110.0); // 100 + 10
    }

    #[test]
    fn test_calculate_text_x_center() {
        let renderer = DefaultTableRenderer;
        let x = renderer.calculate_text_x(&TableAlignment::Center, 100.0, 50.0, 20.0, 10.0);
        assert_eq!(x, 115.0); // 100 + (50 - 20) / 2
    }

    #[test]
    fn test_calculate_text_x_right() {
        let renderer = DefaultTableRenderer;
        let x = renderer.calculate_text_x(&TableAlignment::Right, 100.0, 50.0, 20.0, 10.0);
        assert_eq!(x, 120.0); // 100 + 50 - 10 - 20
    }

    #[test]
    fn test_table_dimensions_empty() {
        let renderer = DefaultTableRenderer;
        let dims = renderer.calculate_dimensions(&[], &TableStyle::default(), 12.0, 400.0);
        assert_eq!(dims.num_cols, 0);
        assert_eq!(dims.num_rows, 0);
    }

    #[test]
    fn test_escape_pdf_string() {
        let helper = PdfTableHelper::default();
        let escaped = helper.escape_pdf_string("test(string)");
        assert_eq!(escaped, "test\\(string\\)");
    }

    #[test]
    fn test_table_style_default() {
        let style = TableStyle::default();
        assert_eq!(style.cell_padding, 8.0);
        assert_eq!(style.margin_top, 16.0);
        assert_eq!(style.border_width, 1.5);
    }
}
