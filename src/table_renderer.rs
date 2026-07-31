//! Table rendering module for PDF generation
//!
//! This module provides a trait-based, modular approach to rendering tables in PDFs.
//! It follows the Strategy pattern for different table rendering approaches.

use crate::elements::TableAlignment;

/// Configuration for table styling
#[derive(Debug, Clone)]
pub struct TableStyle {
    /// Horizontal padding inside each cell (in points)
    pub cell_padding_h: f32,
    /// Vertical padding inside each cell (in points)
    pub cell_padding_v: f32,
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
    /// Header row background color (RGB 0-1). Drawn as a filled rectangle behind the first row.
    pub header_bg_color: Option<(f32, f32, f32)>,
    /// Whether to render the header row text in bold.
    pub header_text_bold: bool,
    /// Enable zebra striping (alternating row background fills).
    pub zebra_striping: bool,
    /// Alternating row background color (RGB 0-1). Used when `zebra_striping` is true.
    pub alt_row_bg_color: (f32, f32, f32),
}

impl Default for TableStyle {
    fn default() -> Self {
        Self {
            cell_padding_h: 8.0,
            cell_padding_v: 8.0,
            margin_top: 16.0,
            margin_bottom: 16.0,
            border_width: 1.5,
            grid_line_width: 0.75,
            border_color: (0.0, 0.0, 0.0),
            grid_color: (0.75, 0.75, 0.75),
            header_bg_color: Some((0.9, 0.9, 0.95)),
            header_text_bold: true,
            zebra_striping: true,
            alt_row_bg_color: (0.96, 0.96, 0.98),
        }
    }
}

impl TableStyle {
    /// Create a style with uniform horizontal and vertical padding.
    pub fn with_padding(padding: f32) -> Self {
        Self {
            cell_padding_h: padding,
            cell_padding_v: padding,
            ..Self::default()
        }
    }
}

/// Represents a single table cell with its content and alignment
#[derive(Debug, Clone)]
pub struct TableCell {
    pub content: String,
    pub alignment: TableAlignment,
    /// Number of columns this cell spans (default 1)
    pub colspan: u32,
    /// Number of rows this cell spans (default 1)
    pub rowspan: u32,
    /// Per-cell horizontal padding override (None = use TableStyle default)
    pub padding_h: Option<f32>,
    /// Per-cell vertical padding override (None = use TableStyle default)
    pub padding_v: Option<f32>,
}

impl TableCell {
    pub fn new(content: String, alignment: TableAlignment) -> Self {
        Self {
            content,
            alignment,
            colspan: 1,
            rowspan: 1,
            padding_h: None,
            padding_v: None,
        }
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

    pub fn justify(content: &str) -> Self {
        Self::new(content.to_string(), TableAlignment::Justify)
    }

    pub fn with_span(mut self, colspan: u32, rowspan: u32) -> Self {
        self.colspan = colspan.max(1);
        self.rowspan = rowspan.max(1);
        self
    }

    pub fn with_padding(mut self, h: f32, v: f32) -> Self {
        self.padding_h = Some(h);
        self.padding_v = Some(v);
        self
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
            cells: cells.iter().map(|s| TableCell::left(s)).collect(),
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

        // Determine the effective grid width accounting for colspans.
        let num_cols = rows
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| c.colspan.max(1) as usize)
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(0);
        let num_rows = rows.len();
        let approx_char_width = base_font_size * 0.5;
        let line_h = base_font_size * 1.4;

        // Calculate column widths — for colspan cells, distribute the cell's
        // width demand evenly across the spanned columns.
        let mut col_widths: Vec<f32> = vec![0.0; num_cols];
        for row in rows {
            let mut col = 0usize;
            for cell in &row.cells {
                let span = cell.colspan.max(1) as usize;
                let pad_h = cell.padding_h.unwrap_or(style.cell_padding_h);
                let cell_width = cell.content.len() as f32 * approx_char_width + pad_h * 2.0;
                let per_col = cell_width / span as f32;
                for s in 0..span {
                    if col + s < num_cols {
                        col_widths[col + s] = col_widths[col + s].max(per_col);
                    }
                }
                col += span;
            }
        }

        // Scale to fit max width
        let total_width: f32 = col_widths.iter().sum();
        if total_width > max_width {
            let scale = max_width / total_width;
            for width in &mut col_widths {
                *width *= scale;
            }
        } else if total_width < max_width && total_width > 0.0 {
            // Expand columns to fill available width proportionally.
            let scale = max_width / total_width;
            for width in &mut col_widths {
                *width *= scale;
            }
        }

        // Calculate row heights — for rowspan cells, distribute the cell's
        // height demand evenly across the spanned rows.
        let mut row_heights: Vec<f32> = vec![0.0; num_rows];
        for (row_idx, row) in rows.iter().enumerate() {
            let mut col = 0usize;
            for cell in &row.cells {
                let cs = cell.colspan.max(1) as usize;
                let rs = cell.rowspan.max(1) as usize;
                let pad_h = cell.padding_h.unwrap_or(style.cell_padding_h);
                let pad_v = cell.padding_v.unwrap_or(style.cell_padding_v);

                // Sum the widths of spanned columns for wrap calculation
                let spanned_width: f32 = (col..col + cs)
                    .take_while(|&i| i < num_cols)
                    .map(|i| col_widths[i])
                    .sum();
                let max_chars = ((spanned_width - pad_h * 2.0) / approx_char_width)
                    .floor()
                    .max(1.0) as usize;
                let wrapped = self.wrap_text(&cell.content, max_chars);
                let cell_height = wrapped.line_count as f32 * line_h + pad_v * 2.0;
                let per_row = cell_height / rs as f32;
                for r in 0..rs {
                    if row_idx + r < num_rows {
                        row_heights[row_idx + r] = row_heights[row_idx + r].max(per_row);
                    }
                }
                col += cs;
            }
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
            TableAlignment::Left | TableAlignment::Justify => cell_x + padding,
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

    /// Convert string rows to TableCell rows with alignments and spans
    pub fn convert_rows(
        &self,
        rows: &[Vec<String>],
        alignments: Option<&[TableAlignment]>,
        colspans: &[Vec<u32>],
        rowspans: &[Vec<u32>],
    ) -> Vec<TableRow> {
        rows.iter()
            .enumerate()
            .map(|(row_idx, row)| {
                let row_colspans = colspans.get(row_idx);
                let row_rowspans = rowspans.get(row_idx);
                let cells: Vec<TableCell> = row
                    .iter()
                    .enumerate()
                    .map(|(col_idx, cell)| {
                        let alignment = alignments
                            .and_then(|a| a.get(col_idx))
                            .copied()
                            .unwrap_or(TableAlignment::Left);
                        let cs = row_colspans
                            .and_then(|c| c.get(col_idx))
                            .copied()
                            .unwrap_or(1)
                            .max(1);
                        let rs = row_rowspans
                            .and_then(|r| r.get(col_idx))
                            .copied()
                            .unwrap_or(1)
                            .max(1);
                        TableCell::new(cell.clone(), alignment)
                            .with_span(cs, rs)
                    })
                    .collect();
                TableRow { cells }
            })
            .collect()
    }

    /// Escape special PDF string characters (public static helper)
    pub fn escape_pdf_string_static(text: &str) -> String {
        crate::pdf_generator::escape_pdf_string(text)
    }

    /// Escape special PDF string characters (instance method)
    pub fn escape_pdf_string(&self, text: &str) -> String {
        Self::escape_pdf_string_static(text)
    }
}

impl Default for PdfTableHelper {
    fn default() -> Self {
        Self::new(Box::new(DefaultTableRenderer))
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
        assert_eq!(style.cell_padding_h, 8.0);
        assert_eq!(style.margin_top, 16.0);
        assert_eq!(style.border_width, 1.5);
    }

    #[test]
    fn test_table_style_header_defaults() {
        let style = TableStyle::default();
        assert!(style.header_bg_color.is_some());
        assert_eq!(style.header_bg_color.unwrap(), (0.9, 0.9, 0.95));
        assert!(style.header_text_bold);
    }

    #[test]
    fn test_table_style_zebra_defaults() {
        let style = TableStyle::default();
        assert!(style.zebra_striping);
        assert_eq!(style.alt_row_bg_color, (0.96, 0.96, 0.98));
    }

    #[test]
    fn test_table_style_disable_zebra() {
        let style = TableStyle {
            zebra_striping: false,
            ..TableStyle::default()
        };
        assert!(!style.zebra_striping);
    }

    #[test]
    fn test_table_style_no_header_bg() {
        let style = TableStyle {
            header_bg_color: None,
            ..TableStyle::default()
        };
        assert!(style.header_bg_color.is_none());
    }

    #[test]
    fn test_table_cell_colspan_rowspan() {
        let cell = TableCell::left("merged").with_span(2, 3);
        assert_eq!(cell.colspan, 2);
        assert_eq!(cell.rowspan, 3);
    }

    #[test]
    fn test_table_cell_per_cell_padding() {
        let cell = TableCell::left("padded").with_padding(4.0, 2.0);
        assert_eq!(cell.padding_h, Some(4.0));
        assert_eq!(cell.padding_v, Some(2.0));
    }

    #[test]
    fn test_table_cell_justify() {
        let cell = TableCell::justify("justified text");
        assert!(matches!(cell.alignment, TableAlignment::Justify));
    }

    #[test]
    fn test_calculate_text_x_justify() {
        let renderer = DefaultTableRenderer;
        let x = renderer.calculate_text_x(&TableAlignment::Justify, 100.0, 50.0, 20.0, 10.0);
        assert_eq!(x, 110.0); // same as Left
    }

    #[test]
    fn test_table_style_split_padding() {
        let style = TableStyle {
            cell_padding_h: 12.0,
            cell_padding_v: 6.0,
            ..TableStyle::default()
        };
        assert_eq!(style.cell_padding_h, 12.0);
        assert_eq!(style.cell_padding_v, 6.0);
    }

    #[test]
    fn test_table_style_with_padding() {
        let style = TableStyle::with_padding(5.0);
        assert_eq!(style.cell_padding_h, 5.0);
        assert_eq!(style.cell_padding_v, 5.0);
    }

    #[test]
    fn test_calculate_dimensions_with_colspan() {
        let renderer = DefaultTableRenderer;
        let rows = vec![TableRow::new(vec![
            TableCell::left("wide").with_span(2, 1),
        ])];
        let dims = renderer.calculate_dimensions(&rows, &TableStyle::default(), 12.0, 400.0);
        assert_eq!(dims.num_cols, 2);
        assert_eq!(dims.num_rows, 1);
    }

    #[test]
    fn test_calculate_dimensions_with_rowspan() {
        let renderer = DefaultTableRenderer;
        let rows = vec![
            TableRow::new(vec![TableCell::left("tall").with_span(1, 2)]),
            TableRow::new(vec![TableCell::left("b")]),
        ];
        let dims = renderer.calculate_dimensions(&rows, &TableStyle::default(), 12.0, 400.0);
        assert_eq!(dims.num_cols, 1);
        assert_eq!(dims.num_rows, 2);
    }

    #[test]
    fn test_convert_rows_with_spans() {
        let helper = PdfTableHelper::default();
        let rows = vec![vec!["a".to_string(), "b".to_string()]];
        let colspans = vec![vec![2, 1]];
        let rowspans = vec![vec![1, 1]];
        let table_rows = helper.convert_rows(&rows, None, &colspans, &rowspans);
        assert_eq!(table_rows[0].cells[0].colspan, 2);
        assert_eq!(table_rows[0].cells[0].rowspan, 1);
    }
}
