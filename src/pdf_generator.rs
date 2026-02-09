use crate::elements::{Element, TextSegment};
use crate::table_renderer::{PdfTableHelper, TableStyle};
use anyhow::Result;
use std::fs::File;
use std::io::Write;
use syntect::parsing::{SyntaxSet, SyntaxReference};

// Lazy static syntax set and theme
fn get_syntax_set() -> &'static SyntaxSet {
    use std::sync::OnceLock;
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(|| {
        SyntaxSet::load_defaults_newlines()
    })
}

fn get_syntax_for_language(lang: &str) -> Option<&'static SyntaxReference> {
    let syntax_set = get_syntax_set();
    match lang.to_lowercase().as_str() {
        "rust" | "rs" => syntax_set.find_syntax_by_token("Rust"),
        "python" | "py" => syntax_set.find_syntax_by_token("Python"),
        "javascript" | "js" => syntax_set.find_syntax_by_token("JavaScript"),
        "typescript" | "ts" => syntax_set.find_syntax_by_token("TypeScript"),
        "html" | "htm" => syntax_set.find_syntax_by_token("HTML"),
        "css" => syntax_set.find_syntax_by_token("CSS"),
        "json" => syntax_set.find_syntax_by_token("JSON"),
        "c" | "cpp" | "cxx" => syntax_set.find_syntax_by_token("C++"),
        "java" => syntax_set.find_syntax_by_token("Java"),
        "go" => syntax_set.find_syntax_by_token("Go"),
        "ruby" => syntax_set.find_syntax_by_token("Ruby"),
        "php" => syntax_set.find_syntax_by_token("PHP"),
        "shell" | "bash" | "sh" => syntax_set.find_syntax_by_token("Bash"),
        "sql" => syntax_set.find_syntax_by_token("SQL"),
        "markdown" | "md" => syntax_set.find_syntax_by_token("Markdown"),
        "xml" => syntax_set.find_syntax_by_token("XML"),
        "yaml" | "yml" => syntax_set.find_syntax_by_token("YAML"),
        _ => syntax_set.find_syntax_by_token("Plain Text"),
    }
}

/// Simple syntax token for rendering (reserved for future use)
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct CodeToken {
    text: String,
    color: Color,
}

/// Perform simple syntax highlighting on code
fn highlight_code(code: &str, language: &str) -> Vec<CodeToken> {
    let syntax_set = get_syntax_set();

    let _syntax = get_syntax_for_language(language)
        .unwrap_or_else(|| syntax_set.find_syntax_by_token("Plain Text").unwrap());

    // Use a simple approach - return tokens with different colors
    // This is a simplified version; full syntect integration would be more complex
    let mut tokens = Vec::new();

    // Basic keyword highlighting for common languages
    let keywords = match language.to_lowercase().as_str() {
        "rust" | "rs" => vec![
            "fn", "let", "mut", "pub", "struct", "enum", "impl", "use", "mod",
            "return", "if", "else", "match", "for", "while", "loop", "break", "continue",
            "true", "false", "const", "static", "trait", "type", "where", "move",
            "crate", "ref", "self", "Self", "super", "async", "await", "unsafe",
        ],
        "python" | "py" => vec![
            "def", "class", "if", "else", "elif", "for", "while", "return",
            "import", "from", "as", "try", "except", "finally", "with", "lambda",
            "True", "False", "None", "and", "or", "not", "in", "is", "pass", "break", "continue",
        ],
        "javascript" | "js" | "typescript" | "ts" => vec![
            "function", "const", "let", "var", "if", "else", "for", "while", "return",
            "import", "export", "default", "from", "as", "class", "extends", "new",
            "true", "false", "null", "undefined", "async", "await", "try", "catch", "finally",
            "typeof", "instanceof", "this", "super",
        ],
        _ => vec![],
    };

    let string_color = Color::rgb(0.15, 0.49, 0.07); // Green for strings
    let keyword_color = Color::rgb(0.53, 0.07, 0.24); // Purple for keywords
    let comment_color = Color::rgb(0.4, 0.4, 0.4); // Gray for comments
    let number_color = Color::rgb(0.15, 0.15, 0.8); // Blue for numbers
    let default_color = Color::black();

    // Simple tokenization - split by common patterns
    let mut remaining = code.to_string();

    while !remaining.is_empty() {
        // Check for string literals
        if remaining.starts_with('"') {
            if let Some(end) = remaining[1..].find('"') {
                let token = &remaining[..end + 2];
                tokens.push(CodeToken {
                    text: token.to_string(),
                    color: string_color,
                });
                remaining = remaining[end + 2..].to_string();
                continue;
            }
        }

        // Check for single quotes
        if remaining.starts_with('\'') {
            if let Some(end) = remaining[1..].find('\'') {
                let token = &remaining[..end + 2];
                tokens.push(CodeToken {
                    text: token.to_string(),
                    color: string_color,
                });
                remaining = remaining[end + 2..].to_string();
                continue;
            }
        }

        // Check for comments
        if remaining.starts_with("//") {
            if let Some(end) = remaining.find('\n') {
                let token = &remaining[..end];
                tokens.push(CodeToken {
                    text: token.to_string(),
                    color: comment_color,
                });
                remaining = remaining[end..].to_string();
                continue;
            } else {
                tokens.push(CodeToken {
                    text: remaining.clone(),
                    color: comment_color,
                });
                break;
            }
        }

        // Check for comments (hash style)
        if remaining.starts_with('#') {
            if let Some(end) = remaining.find('\n') {
                let token = &remaining[..end];
                tokens.push(CodeToken {
                    text: token.to_string(),
                    color: comment_color,
                });
                remaining = remaining[end..].to_string();
                continue;
            } else {
                tokens.push(CodeToken {
                    text: remaining.clone(),
                    color: comment_color,
                });
                break;
            }
        }

        // Check for numbers
        if remaining.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            let end = remaining.chars()
                .position(|c| !c.is_ascii_digit() && c != '.')
                .unwrap_or(remaining.len());
            let token = &remaining[..end];
            tokens.push(CodeToken {
                text: token.to_string(),
                color: number_color,
            });
            remaining = remaining[end..].to_string();
            continue;
        }

        // Check for keywords
        let mut found_keyword = false;
        for keyword in &keywords {
            if remaining.starts_with(keyword) {
                let next_char = remaining.chars().nth(keyword.len());
                if next_char.map(|c| !c.is_alphanumeric() && c != '_').unwrap_or(true) {
                    tokens.push(CodeToken {
                        text: keyword.to_string(),
                        color: keyword_color,
                    });
                    remaining = remaining[keyword.len()..].to_string();
                    found_keyword = true;
                    break;
                }
            }
        }

        if found_keyword {
            continue;
        }

        // Take a run of plain characters (identifiers, whitespace, punctuation)
        // until we hit something that could start a special token
        let mut end = 0;
        let mut chars_iter = remaining.chars();
        while let Some(c) = chars_iter.next() {
            let rest = &remaining[end..];
            // Stop if we see the start of a string, comment, number-at-word-boundary, or keyword
            if end > 0 && (c == '"' || c == '\''
                || rest.starts_with("//")
                || (c == '#' && !remaining[..end].ends_with(|ch: char| ch.is_alphanumeric() || ch == '_'))
                || (c.is_ascii_digit() && (end == 0 || !remaining.as_bytes().get(end.wrapping_sub(1)).map(|b| b.is_ascii_alphanumeric() || *b == b'_').unwrap_or(false))))
            {
                break;
            }
            // Check if a keyword starts here (only at word boundary)
            let mut is_keyword_start = false;
            if end > 0 {
                let prev = remaining.as_bytes()[end - 1];
                if !prev.is_ascii_alphanumeric() && prev != b'_' {
                    for keyword in &keywords {
                        if rest.starts_with(keyword) {
                            let next = rest.chars().nth(keyword.len());
                            if next.map(|nc| !nc.is_alphanumeric() && nc != '_').unwrap_or(true) {
                                is_keyword_start = true;
                                break;
                            }
                        }
                    }
                }
            }
            if is_keyword_start {
                break;
            }
            end += c.len_utf8();
        }
        if end == 0 {
            // Couldn't group, take one character
            let c = remaining.chars().next().unwrap();
            end = c.len_utf8();
        }
        let chunk = &remaining[..end];
        tokens.push(CodeToken {
            text: chunk.to_string(),
            color: default_color,
        });
        remaining = remaining[end..].to_string();
    }

    // If tokenization failed, just return the whole code as one token
    if tokens.is_empty() && !code.is_empty() {
        tokens.push(CodeToken {
            text: code.to_string(),
            color: default_color,
        });
    }

    tokens
}

// --- Page orientation and layout ---

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageOrientation {
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy)]
pub struct PageLayout {
    pub width: f32,
    pub height: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
}

impl PageLayout {
    pub fn portrait() -> Self {
        PageLayout {
            width: 612.0,
            height: 792.0,
            margin_left: 72.0,
            margin_right: 72.0,
            margin_top: 72.0,
            margin_bottom: 72.0,
        }
    }

    pub fn landscape() -> Self {
        PageLayout {
            width: 792.0,
            height: 612.0,
            margin_left: 72.0,
            margin_right: 72.0,
            margin_top: 72.0,
            margin_bottom: 72.0,
        }
    }

    pub fn from_orientation(orientation: PageOrientation) -> Self {
        match orientation {
            PageOrientation::Portrait => Self::portrait(),
            PageOrientation::Landscape => Self::landscape(),
        }
    }

    pub fn content_top(&self) -> f32 {
        self.height - self.margin_top
    }

    pub fn content_width(&self) -> f32 {
        self.width - self.margin_left - self.margin_right
    }
}

// --- Font size helpers ---
fn heading_font_size(level: u8, base: f32) -> f32 {
    match level {
        1 => base * 2.0,
        2 => base * 1.6,
        3 => base * 1.3,
        4 => base * 1.1,
        5 => base * 1.0,
        _ => base * 0.9,
    }
}

fn line_height(font_size: f32) -> f32 {
    font_size + 4.0
}

// --- Low-level PDF object model ---

pub struct PdfGenerator {
    pub objects: Vec<PdfObj>,
    pub next_id: u32,
}

#[derive(Debug)]
pub struct PdfObj {
    pub id: u32,
    pub generation: u32,
    pub content: String,
    pub is_stream: bool,
    pub stream_data: Option<Vec<u8>>,
}

impl PdfGenerator {
    pub fn new() -> Self {
        PdfGenerator {
            objects: Vec::new(),
            next_id: 1,
        }
    }

    pub fn add_object(&mut self, content: String) -> u32 {
        let id = self.next_id;
        self.objects.push(PdfObj {
            id,
            generation: 0,
            content,
            is_stream: false,
            stream_data: None,
        });
        self.next_id += 1;
        id
    }

    pub fn add_stream_object(&mut self, dictionary: String, data: Vec<u8>) -> u32 {
        let id = self.next_id;
        self.objects.push(PdfObj {
            id,
            generation: 0,
            content: dictionary,
            is_stream: true,
            stream_data: Some(data),
        });
        self.next_id += 1;
        id
    }

    pub fn generate(&self) -> Vec<u8> {
        let mut pdf = Vec::new();

        // PDF header
        pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

        // Calculate offsets for xref table
        let mut offsets = Vec::new();
        let mut current_offset = pdf.len() as u32;

        // Write objects and collect offsets
        for obj in &self.objects {
            offsets.push(current_offset);
            let obj_header = format!("{} {} obj\n", obj.id, obj.generation);
            pdf.extend_from_slice(obj_header.as_bytes());
            pdf.extend_from_slice(obj.content.as_bytes());

            if obj.is_stream {
                if let Some(data) = &obj.stream_data {
                    pdf.extend_from_slice(b"stream\n");
                    pdf.extend_from_slice(data);
                    pdf.extend_from_slice(b"\nendstream\n");
                }
            }

            pdf.extend_from_slice(b"endobj\n");
            current_offset = pdf.len() as u32;
        }

        // xref table
        let xref_offset = pdf.len() as u32;
        pdf.extend_from_slice(format!("xref\n0 {}\n", self.objects.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");

        for offset in offsets {
            pdf.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
        }

        // trailer
        pdf.extend_from_slice(b"trailer\n");
        pdf.extend_from_slice(b"<<\n");
        pdf.extend_from_slice(format!("/Size {}\n", self.objects.len() + 1).as_bytes());
        if !self.objects.is_empty() {
            pdf.extend_from_slice(format!("/Root {} 0 R\n", self.objects.len()).as_bytes());
        }
        pdf.extend_from_slice(b">>\n");
        pdf.extend_from_slice(b"startxref\n");
        pdf.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        pdf.extend_from_slice(b"%%EOF\n");

        pdf
    }
}

// --- Content stream builder (handles cursor, page breaks, font switches) ---

/// RGB color for text rendering (0.0-1.0 per channel)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub fn black() -> Self { Color { r: 0.0, g: 0.0, b: 0.0 } }
    pub fn red() -> Self { Color { r: 1.0, g: 0.0, b: 0.0 } }
    pub fn blue() -> Self { Color { r: 0.0, g: 0.0, b: 1.0 } }
    pub fn gray() -> Self { Color { r: 0.5, g: 0.5, b: 0.5 } }
    pub fn rgb(r: f32, g: f32, b: f32) -> Self { Color { r, g, b } }
}

/// Text alignment for line rendering
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

struct ContentStreamBuilder {
    pages: Vec<Vec<u8>>,
    current: Vec<u8>,
    y: f32,
    base_font_size: f32,
    current_font_size: f32,
    current_color: Color,
    page_number: u32,
    total_pages: u32,
    show_page_numbers: bool,
    layout: PageLayout,
    // Font state
    current_font: String,  // Font name (e.g., "Helvetica", "Helvetica-Bold")
    current_font_bold: bool,
    current_font_italic: bool,
}

// Font name constants
const FONT_HELVETICA: &str = "Helvetica";
const FONT_HELVETICA_BOLD: &str = "Helvetica-Bold";
const FONT_HELVETICA_OBLIQUE: &str = "Helvetica-Oblique";
const FONT_HELVETICA_BOLD_OBLIQUE: &str = "Helvetica-BoldOblique";
const FONT_COURIER: &str = "Courier";  // Monospace for code

impl ContentStreamBuilder {
    fn new(base_font_size: f32, show_page_numbers: bool, layout: PageLayout) -> Self {
        let mut b = ContentStreamBuilder {
            pages: Vec::new(),
            current: Vec::new(),
            y: layout.content_top(),
            base_font_size,
            current_font_size: base_font_size,
            current_color: Color::black(),
            page_number: 1,
            total_pages: 0,
            show_page_numbers,
            layout,
            current_font: FONT_HELVETICA.to_string(),
            current_font_bold: false,
            current_font_italic: false,
        };
        b.begin_page();
        b
    }

    fn begin_page(&mut self) {
        self.current.clear();
        self.y = self.layout.content_top();
        self.current.extend_from_slice(b"BT\n");
        self.set_font_with_style(self.base_font_size, false, false);
    }

    fn set_font(&mut self, size: f32) {
        self.set_font_with_style(size, self.current_font_bold, self.current_font_italic);
    }

    fn set_font_with_style(&mut self, size: f32, bold: bool, italic: bool) {
        self.current_font_size = size;
        self.current_font_bold = bold;
        self.current_font_italic = italic;

        let font_name = match (bold, italic) {
            (true, true) => FONT_HELVETICA_BOLD_OBLIQUE,
            (true, false) => FONT_HELVETICA_BOLD,
            (false, true) => FONT_HELVETICA_OBLIQUE,
            (false, false) => FONT_HELVETICA,
        };

        if self.current_font != font_name {
            self.current_font = font_name.to_string();
        }

        // Use the current font
        self.current
            .extend_from_slice(format!("/{} {} Tf\n", font_name, size).as_bytes());
    }

    fn set_monospace_font(&mut self, size: f32) {
        self.current_font_size = size;
        self.current_font = FONT_COURIER.to_string();
        self.current
            .extend_from_slice(format!("/{} {} Tf\n", FONT_COURIER, size).as_bytes());
    }

    fn draw_rectangle(&mut self, x: f32, y: f32, width: f32, height: f32, fill_color: Color) {
        // End text block temporarily to draw rectangle
        self.current.extend_from_slice(b"ET\n");

        // Set fill color
        self.current.extend_from_slice(
            format!("{} {} {} rg\n", fill_color.r, fill_color.g, fill_color.b).as_bytes()
        );

        // Draw and fill rectangle
        self.current.extend_from_slice(
            format!("{} {} {} {} re f\n", x, y, width, height).as_bytes()
        );

        // Resume text block
        self.current.extend_from_slice(b"BT\n");
        self.set_font(self.current_font_size);
        // Always reset to black text after drawing rectangle
        self.current_color = Color::black();
        self.current.extend_from_slice(
            format!("0 0 0 rg\n").as_bytes()
        );
    }

    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, line_width: f32, color: Color) {
        // End text block temporarily to draw line
        self.current.extend_from_slice(b"ET\n");

        // Set stroke color and line width
        self.current.extend_from_slice(
            format!("{} {} {} RG\n", color.r, color.g, color.b).as_bytes()
        );
        self.current.extend_from_slice(
            format!("{} w\n", line_width).as_bytes()
        );

        // Draw line
        self.current.extend_from_slice(
            format!("{} {} m {} {} l S\n", x1, y1, x2, y2).as_bytes()
        );

        // Resume text block
        self.current.extend_from_slice(b"BT\n");
        self.set_font(self.current_font_size);
        // Reset to current text color
        self.current.extend_from_slice(
            format!("{} {} {} rg\n", self.current_color.r, self.current_color.g, self.current_color.b).as_bytes()
        );
    }

    /// Render a complete table with borders, text wrapping, and alignment
    fn render_table(&mut self, rows: &[Vec<String>], base_font_size: f32, alignments: Option<&[crate::elements::TableAlignment]>) {
        if rows.is_empty() {
            return;
        }

        let table_helper = PdfTableHelper::default();
        let style = TableStyle::default();

        // Convert string rows to TableRow with alignments
        let table_rows = table_helper.convert_rows(rows, alignments);

        // Calculate table dimensions
        let dims = table_helper.renderer().calculate_dimensions(
            &table_rows,
            &style,
            base_font_size,
            self.layout.content_width(),
        );

        if dims.num_cols == 0 || dims.num_rows == 0 {
            return;
        }

        let line_h = line_height(base_font_size);
        let approx_char_width = base_font_size * 0.5;

        // Add margin above table
        self.y -= style.margin_top;

        // Check for page break
        if self.needs_page_break(dims.total_height + style.margin_top + style.margin_bottom) {
            self.new_page();
            self.y -= style.margin_top;
        }

        let start_x = self.layout.margin_left;
        let start_y = self.y;

        // Draw outer border
        self.current.extend_from_slice(b"ET\n");
        let (br, bg, bb) = style.border_color;
        self.current.extend_from_slice(
            format!("{} {} {} RG\n", br, bg, bb).as_bytes()
        );
        self.current.extend_from_slice(
            format!("{} w\n", style.border_width).as_bytes()
        );
        self.current.extend_from_slice(
            format!("{} {} m {} {} l S\n", start_x, start_y, start_x + dims.total_width, start_y).as_bytes()
        );
        self.current.extend_from_slice(
            format!("{} {} m {} {} l S\n", start_x, start_y - dims.total_height, start_x + dims.total_width, start_y - dims.total_height).as_bytes()
        );
        self.current.extend_from_slice(
            format!("{} {} m {} {} l S\n", start_x, start_y, start_x, start_y - dims.total_height).as_bytes()
        );
        self.current.extend_from_slice(
            format!("{} {} m {} {} l S\n", start_x + dims.total_width, start_y, start_x + dims.total_width, start_y - dims.total_height).as_bytes()
        );

        // Draw horizontal grid lines
        let mut current_y = start_y;
        for (i, &row_h) in dims.row_heights.iter().enumerate() {
            if i > 0 {
                let (gr, gg, gb) = style.grid_color;
                self.current.extend_from_slice(
                    format!("{} {} {} RG\n", gr, gg, gb).as_bytes()
                );
                self.current.extend_from_slice(
                    format!("{} w\n", style.grid_line_width).as_bytes()
                );
                self.current.extend_from_slice(
                    format!("{} {} m {} {} l S\n", start_x, current_y, start_x + dims.total_width, current_y).as_bytes()
                );
            }
            current_y -= row_h;
        }

        // Draw vertical grid lines
        let mut current_x = start_x;
        for i in 1..dims.num_cols {
            current_x += dims.column_widths[i - 1];
            let (gr, gg, gb) = style.grid_color;
            self.current.extend_from_slice(
                format!("{} {} {} RG\n", gr, gg, gb).as_bytes()
            );
            self.current.extend_from_slice(
                format!("{} w\n", style.grid_line_width).as_bytes()
            );
            self.current.extend_from_slice(
                format!("{} {} m {} {} l S\n", current_x, start_y, current_x, start_y - dims.total_height).as_bytes()
            );
        }

        // Resume text block
        self.current.extend_from_slice(b"BT\n");
        self.set_font(base_font_size);
        self.current.extend_from_slice(b"0 0 0 rg\n");

        // Draw cell contents with wrapping and alignment
        let mut row_y = start_y;
        for (row_idx, row) in table_rows.iter().enumerate() {
            let mut col_x = start_x;
            for (col_idx, cell) in row.cells.iter().enumerate() {
                if col_idx >= dims.num_cols { break; }
                let cell_width = dims.column_widths[col_idx];
                let cell_height = dims.row_heights[row_idx];
                let max_chars = ((cell_width - style.cell_padding * 2.0) / approx_char_width).floor().max(1.0) as usize;

                // Wrap text into lines using the table helper
                let wrapped = table_helper.renderer().wrap_text(&cell.content, max_chars);

                // Calculate vertical centering
                let text_height = wrapped.line_count as f32 * line_h;
                let start_y_pos = row_y - (cell_height - text_height) / 2.0 - line_h / 3.0;

                // Render each line with proper alignment
                for (line_idx, line) in wrapped.lines.iter().enumerate() {
                    let line_width = line.len() as f32 * approx_char_width;

                    // Calculate X position using the table helper
                    let x = table_helper.renderer().calculate_text_x(
                        &cell.alignment,
                        col_x,
                        cell_width,
                        line_width,
                        style.cell_padding,
                    );

                    let y = start_y_pos - (line_idx as f32 * line_h);

                    self.current.extend_from_slice(
                        format!("1 0 0 1 {} {} Tm\n", x, y).as_bytes()
                    );
                    self.current.extend_from_slice(
                        format!("({}) Tj\n", PdfTableHelper::escape_pdf_string_static(line)).as_bytes()
                    );
                }

                col_x += cell_width;
            }
            row_y -= dims.row_heights[row_idx];
        }

        self.y -= dims.total_height + style.margin_bottom;
    }

    /// Approximate text width for wrapping calculations
    fn estimate_text_width(&self, text: &str, font_size: f32) -> f32 {
        // Rough approximation: average character width is 0.5 * font_size
        // For monospace (Courier), it's closer to 0.6 * font_size
        let multiplier = if self.current_font == FONT_COURIER { 0.6 } else { 0.5 };
        text.len() as f32 * font_size * multiplier
    }

    /// Emit wrapped text that fits within the content width
    fn emit_wrapped_text(&mut self, text: &str, font_size: f32) {
        let max_width = self.layout.content_width();
        let approx_char_width = font_size * 0.5;
        let max_chars = (max_width / approx_char_width).floor() as usize;

        if text.len() <= max_chars {
            self.emit_line(text, font_size);
            return;
        }

        // Simple word wrapping
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut current_line = String::new();

        for word in words {
            let test_line = if current_line.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", current_line, word)
            };

            if test_line.len() <= max_chars {
                current_line = test_line;
            } else {
                if !current_line.is_empty() {
                    self.emit_line(&current_line, font_size);
                }
                current_line = word.to_string();
            }
        }

        if !current_line.is_empty() {
            self.emit_line(&current_line, font_size);
        }
    }

    fn set_color(&mut self, color: Color) {
        if self.current_color != color {
            self.current_color = color;
            self.current
                .extend_from_slice(format!("{} {} {} rg\n", color.r, color.g, color.b).as_bytes());
        }
    }

    fn reset_color(&mut self) {
        self.set_color(Color::black());
    }

    fn needs_page_break(&self, extra: f32) -> bool {
        self.y - extra < self.layout.margin_bottom
    }

    fn new_page(&mut self) {
        self.end_text_block();
        self.pages.push(self.current.clone());
        self.page_number += 1;
        self.begin_page();
    }

    fn end_text_block(&mut self) {
        self.current.extend_from_slice(b"ET\n");
        if self.show_page_numbers {
            self.write_page_number();
        }
    }

    fn write_page_number(&mut self) {
        let label = format!("Page {}", self.page_number);
        let x = self.layout.width / 2.0 - 20.0;
        let y = self.layout.margin_bottom / 2.0;
        self.current.extend_from_slice(b"BT\n");
        self.current
            .extend_from_slice(format!("/F1 9 Tf\n").as_bytes());
        self.current
            .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, y).as_bytes());
        self.current
            .extend_from_slice(format!("({}) Tj\n", escape_pdf_string(&label)).as_bytes());
        self.current.extend_from_slice(b"ET\n");
    }

    fn emit_line(&mut self, text: &str, font_size: f32) {
        self.emit_line_aligned(text, font_size, TextAlign::Left);
    }

    fn emit_line_aligned(&mut self, text: &str, font_size: f32, align: TextAlign) {
        let lh = line_height(font_size);
        if self.needs_page_break(lh) {
            self.new_page();
        }
        self.set_font(font_size);
        let escaped = escape_pdf_string(text);

        let x = match align {
            TextAlign::Left => self.layout.margin_left,
            TextAlign::Center => {
                // Approximate: 0.5 * char_count * font_size * 0.5
                let approx_width = text.len() as f32 * font_size * 0.5;
                self.layout.margin_left + (self.layout.content_width() - approx_width) / 2.0
            }
            TextAlign::Right => {
                // Approximate: 0.5 * char_count * font_size * 0.5
                let approx_width = text.len() as f32 * font_size * 0.5;
                self.layout.margin_left + self.layout.content_width() - approx_width
            }
            TextAlign::Justify => {
                // Justify is similar to left for positioning, but would adjust word spacing
                // For simplicity, we treat it like left for now
                self.layout.margin_left
            }
        };

        // Use Tm (text matrix) for absolute positioning — Td is relative and compounds
        self.current
            .extend_from_slice(format!("1 0 0 1 {} {} Tm\n", x, self.y).as_bytes());
        self.current
            .extend_from_slice(format!("({}) Tj\n", escaped).as_bytes());
        self.y -= lh;
    }

    fn emit_colored_line(&mut self, text: &str, font_size: f32, color: Color) {
        self.set_color(color);
        self.emit_line(text, font_size);
        self.reset_color();
    }

    fn emit_empty_line(&mut self) {
        let lh = line_height(self.base_font_size) * 0.5;
        if self.needs_page_break(lh) {
            self.new_page();
        }
        self.y -= lh;
    }

    fn emit_horizontal_rule(&mut self) {
        // Add spacing above the rule
        self.y -= line_height(self.base_font_size) / 2.0;

        // Check for page break
        if self.needs_page_break(line_height(self.base_font_size)) {
            self.new_page();
        }

        // Draw a horizontal line across the content area
        let x1 = self.layout.margin_left;
        let x2 = self.layout.margin_left + self.layout.content_width();
        let y = self.y;
        let line_width = 1.0;
        let color = Color::gray();

        self.draw_line(x1, y, x2, y, line_width, color);

        // Add spacing below the rule
        self.y -= line_height(self.base_font_size);
    }

    fn finish(mut self) -> Vec<Vec<u8>> {
        self.end_text_block();
        self.pages.push(self.current);
        self.pages
    }
}

// --- Public API ---

pub fn create_pdf(filename: &str, text: &str) -> Result<()> {
    create_pdf_with_options(filename, text, "Helvetica", 12.0)
}

/// Legacy plain-text pipeline (backward compatible)
pub fn create_pdf_with_options(
    filename: &str,
    text: &str,
    font: &str,
    font_size: f32,
) -> Result<()> {
    let elements: Vec<Element> = text
        .lines()
        .map(|l| {
            if l.trim().is_empty() {
                Element::EmptyLine
            } else {
                Element::Paragraph {
                    text: l.to_string(),
                }
            }
        })
        .collect();
    create_pdf_from_elements(filename, &elements, font, font_size)
}

/// Rich element-based pipeline with header sizes, page numbers, etc.
pub fn create_pdf_from_elements(
    filename: &str,
    elements: &[Element],
    font: &str,
    base_font_size: f32,
) -> Result<()> {
    create_pdf_from_elements_with_layout(filename, elements, font, base_font_size, PageLayout::portrait())
}

/// Rich element-based pipeline with configurable page layout (orientation)
pub fn create_pdf_from_elements_with_layout(
    filename: &str,
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
) -> Result<()> {
    let show_page_numbers = true;
    let mut builder = ContentStreamBuilder::new(base_font_size, show_page_numbers, layout);
    render_elements_to_builder(&mut builder, elements, base_font_size);
    let page_streams = builder.finish();
    assemble_pdf(filename, &page_streams, font, &layout)?;
    Ok(())
}

/// Render elements into a ContentStreamBuilder (shared by file and bytes APIs)
fn render_elements_to_builder(builder: &mut ContentStreamBuilder, elements: &[Element], base_font_size: f32) {
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut table_alignments: Option<Vec<crate::elements::TableAlignment>> = None;

    for elem in elements {
        // Handle table rows specially - accumulate them
        if let Element::TableRow { cells, is_separator, alignments } = elem {
            if *is_separator {
                // Store alignments from separator row
                table_alignments = Some(alignments.clone());
            } else {
                // Only add non-separator rows to the table
                table_rows.push(cells.clone());
            }
            continue;
        }

        // Flush any accumulated table before rendering non-table element
        if !table_rows.is_empty() {
            builder.render_table(&table_rows, base_font_size, table_alignments.as_deref());
            table_rows.clear();
            table_alignments = None;
        }

        // Render non-table elements
        match elem {
            Element::Heading { level, text } => {
                let fs = heading_font_size(*level, base_font_size);
                let align = if *level == 1 { TextAlign::Center } else { TextAlign::Left };
                builder.emit_empty_line();
                builder.set_font_with_style(fs, true, false);
                builder.emit_line_aligned(text, fs, align);
                builder.set_font_with_style(base_font_size, false, false);
                builder.emit_empty_line();
            }
            Element::Paragraph { text } => {
                builder.emit_wrapped_text(text, base_font_size);
            }
            Element::RichParagraph { segments } => {
                // Render each styled segment
                for segment in segments {
                    match segment {
                        TextSegment::Plain(text) => {
                            builder.set_font_with_style(base_font_size, false, false);
                            builder.emit_wrapped_text(text, base_font_size);
                        }
                        TextSegment::Bold(text) => {
                            builder.set_font_with_style(base_font_size, true, false);
                            builder.emit_wrapped_text(text, base_font_size);
                        }
                        TextSegment::Italic(text) => {
                            builder.set_font_with_style(base_font_size, false, true);
                            builder.emit_wrapped_text(text, base_font_size);
                        }
                        TextSegment::BoldItalic(text) => {
                            builder.set_font_with_style(base_font_size, true, true);
                            builder.emit_wrapped_text(text, base_font_size);
                        }
                        TextSegment::Code(code) => {
                            let code_size = base_font_size * 0.9;
                            builder.set_monospace_font(code_size);
                            builder.set_color(Color::gray());
                            builder.emit_wrapped_text(code, code_size);
                            builder.set_color(Color::black());
                            builder.set_font_with_style(base_font_size, false, false);
                        }
                        TextSegment::Link { text, url } => {
                            builder.set_color(Color::blue());
                            builder.emit_wrapped_text(&format!("{} ({})", text, url), base_font_size);
                            builder.set_color(Color::black());
                        }
                    }
                }
            }
            Element::UnorderedListItem { text, depth } => {
                let indent = "  ".repeat(*depth as usize);
                let line = format!("{}• {}", indent, text);
                builder.emit_wrapped_text(&line, base_font_size);
            }
            Element::OrderedListItem { number, text, depth } => {
                let indent = "  ".repeat(*depth as usize);
                let line = format!("{}{}. {}", indent, number, text);
                builder.emit_wrapped_text(&line, base_font_size);
            }
            Element::TaskListItem { checked, text } => {
                let marker = if *checked { "[x]" } else { "[ ]" };
                let line = format!("{} {}", marker, text);
                builder.emit_wrapped_text(&line, base_font_size);
            }
            Element::CodeBlock { code, language } => {
                let code_size = base_font_size * 0.85;
                let padding = 8.0;
                let line_h = line_height(code_size);
                let all_lines: Vec<&str> = code.lines().collect();

                builder.emit_empty_line();

                // Split code block across pages if needed
                let mut line_idx = 0;
                while line_idx < all_lines.len() {
                    // Calculate how many lines fit on current page
                    let available = builder.y - builder.layout.margin_bottom - padding * 2.0;
                    let max_lines_on_page = (available / line_h).floor() as usize;
                    let max_lines_on_page = max_lines_on_page.max(1);
                    let chunk_end = (line_idx + max_lines_on_page).min(all_lines.len());
                    let chunk = &all_lines[line_idx..chunk_end];
                    let chunk_height = chunk.len() as f32 * line_h + padding * 2.0;

                    // Account for top padding before drawing — shift y down by padding
                    builder.y -= padding;

                    // Draw background rectangle (from current y down by text height + bottom padding)
                    let text_block_height = chunk.len() as f32 * line_h;
                    let bg_color = Color::rgb(0.95, 0.95, 0.95);
                    let rect_x = builder.layout.margin_left - padding;
                    let rect_y = builder.y - text_block_height - padding;
                    let rect_width = builder.layout.content_width() + padding * 2.0;
                    let rect_height = chunk_height;
                    builder.draw_rectangle(rect_x, rect_y, rect_width, rect_height, bg_color);

                    // Draw border
                    let border_color = Color::rgb(0.75, 0.75, 0.75);
                    builder.draw_line(rect_x, rect_y, rect_x + rect_width, rect_y, 0.5, border_color);
                    builder.draw_line(rect_x, rect_y + rect_height, rect_x + rect_width, rect_y + rect_height, 0.5, border_color);
                    builder.draw_line(rect_x, rect_y, rect_x, rect_y + rect_height, 0.5, border_color);
                    builder.draw_line(rect_x + rect_width, rect_y, rect_x + rect_width, rect_y + rect_height, 0.5, border_color);

                    // Set monospace font
                    builder.set_monospace_font(code_size);

                    // Emit code lines with per-line syntax highlighting
                    let char_width = code_size * 0.6; // Courier is monospace
                    for code_line in chunk {
                        let line_tokens = highlight_code(code_line, language);

                        if line_tokens.is_empty() || line_tokens.iter().all(|t| t.text.is_empty()) {
                            // Empty line or no tokens — just advance
                            builder.current.extend_from_slice(
                                format!("{} {} {} rg\n", 0.15, 0.15, 0.15).as_bytes()
                            );
                            builder.current.extend_from_slice(
                                format!("1 0 0 1 {} {} Tm\n", builder.layout.margin_left, builder.y).as_bytes()
                            );
                            builder.current.extend_from_slice(
                                format!("({}) Tj\n", escape_pdf_string(code_line)).as_bytes()
                            );
                        } else {
                            // Render each token with its color
                            let mut x_offset = builder.layout.margin_left;
                            for token in &line_tokens {
                                if token.text.is_empty() { continue; }
                                builder.current.extend_from_slice(
                                    format!("{} {} {} rg\n", token.color.r, token.color.g, token.color.b).as_bytes()
                                );
                                builder.current.extend_from_slice(
                                    format!("1 0 0 1 {} {} Tm\n", x_offset, builder.y).as_bytes()
                                );
                                builder.current.extend_from_slice(
                                    format!("({}) Tj\n", escape_pdf_string(&token.text)).as_bytes()
                                );
                                x_offset += token.text.len() as f32 * char_width;
                            }
                        }
                        builder.y -= line_h;
                    }

                    // Account for bottom padding
                    builder.y -= padding;

                    line_idx = chunk_end;

                    // If more lines remain, start a new page
                    if line_idx < all_lines.len() {
                        builder.set_font_with_style(base_font_size, false, false);
                        builder.reset_color();
                        builder.new_page();
                    }
                }

                // Reset to normal font and color
                builder.set_font_with_style(base_font_size, false, false);
                builder.reset_color();
                builder.emit_empty_line();
            }
            Element::DefinitionItem { term, definition } => {
                builder.set_font_with_style(base_font_size, true, false);
                builder.emit_wrapped_text(term, base_font_size);
                builder.set_font_with_style(base_font_size, false, false);
                builder.emit_wrapped_text(&format!("  {}", definition), base_font_size);
            }
            Element::InlineCode { code } => {
                let code_size = base_font_size * 0.9;
                builder.set_monospace_font(code_size);
                builder.set_color(Color::gray());
                builder.emit_line(code, code_size);
                builder.set_font_with_style(base_font_size, false, false);
                builder.reset_color();
            }
            Element::Link { text, url } => {
                builder.set_color(Color::blue());
                builder.emit_wrapped_text(&format!("{} ({})", text, url), base_font_size);
                builder.reset_color();
            }
            Element::Image { alt, path } => {
                builder.emit_wrapped_text(&format!("[Image: {}] ({})", alt, path), base_font_size);
            }
            Element::StyledText { text, bold, italic } => {
                builder.set_font_with_style(base_font_size, *bold, *italic);
                builder.emit_wrapped_text(text, base_font_size);
                builder.set_font_with_style(base_font_size, false, false);
            }
            Element::PageBreak => {
                builder.new_page();
            }
            Element::Footnote { label, text } => {
                let footnote_size = base_font_size * 0.85;
                builder.emit_wrapped_text(&format!("[{}] {}", label, text), footnote_size);
            }
            Element::BlockQuote { text, depth } => {
                let prefix = "> ".repeat(*depth as usize);
                builder.set_color(Color::gray());
                builder.emit_wrapped_text(&format!("{}{}", prefix, text), base_font_size);
                builder.reset_color();
            }
            Element::MathBlock { expression } => {
                let math_size = base_font_size * 1.1;
                let padding = 10.0;
                let line_h = line_height(math_size);
                let math_lines: Vec<&str> = expression.lines().collect();
                let block_height = math_lines.len() as f32 * line_h + padding * 2.0;

                builder.emit_empty_line();

                // Check page break
                if builder.needs_page_break(block_height) {
                    builder.new_page();
                }

                // Draw light blue background
                let bg_color = Color::rgb(0.93, 0.95, 1.0);
                let rect_x = builder.layout.margin_left - padding;
                let rect_y = builder.y - block_height;
                let rect_width = builder.layout.content_width() + padding * 2.0;
                builder.draw_rectangle(rect_x, rect_y, rect_width, block_height, bg_color);

                // Draw left accent border
                let accent_color = Color::rgb(0.3, 0.4, 0.8);
                builder.draw_line(rect_x, rect_y, rect_x, rect_y + block_height, 2.0, accent_color);

                // Render math expression in italic
                builder.set_font_with_style(math_size, false, true);
                builder.set_color(Color::rgb(0.1, 0.1, 0.3));
                for math_line in &math_lines {
                    // Render math symbols with text representation
                    let rendered = render_math_text(math_line);
                    builder.current.extend_from_slice(
                        format!("1 0 0 1 {} {} Tm\n", builder.layout.margin_left + 4.0, builder.y).as_bytes()
                    );
                    builder.current.extend_from_slice(
                        format!("({}) Tj\n", escape_pdf_string(&rendered)).as_bytes()
                    );
                    builder.y -= line_h;
                }

                builder.set_font_with_style(base_font_size, false, false);
                builder.reset_color();
                builder.emit_empty_line();
            }
            Element::MathInline { expression } => {
                // Render inline math in italic with slight color
                let rendered = render_math_text(expression);
                builder.set_font_with_style(base_font_size, false, true);
                builder.set_color(Color::rgb(0.1, 0.1, 0.3));
                builder.emit_line(&rendered, base_font_size);
                builder.set_font_with_style(base_font_size, false, false);
                builder.reset_color();
            }
            Element::HorizontalRule => {
                builder.emit_horizontal_rule();
            }
            Element::EmptyLine => {
                builder.emit_empty_line();
            }
            Element::TableRow { .. } => {
                // Already handled above
            }
        }
    }

    // Flush any remaining table
    if !table_rows.is_empty() {
        builder.render_table(&table_rows, base_font_size, table_alignments.as_deref());
    }
}

/// Generate PDF bytes from elements (library API — no filesystem access needed)
pub fn generate_pdf_bytes(
    elements: &[Element],
    font: &str,
    base_font_size: f32,
    layout: PageLayout,
) -> Result<Vec<u8>> {
    let show_page_numbers = true;
    let mut builder = ContentStreamBuilder::new(base_font_size, show_page_numbers, layout);
    render_elements_to_builder(&mut builder, elements, base_font_size);
    let page_streams = builder.finish();
    Ok(assemble_pdf_bytes(&page_streams, font, &layout))
}

/// Assemble final PDF bytes from per-page content streams
fn assemble_pdf_bytes(page_streams: &[Vec<u8>], _font: &str, layout: &PageLayout) -> Vec<u8> {
    let mut generator = PdfGenerator::new();

    let mut page_ids = Vec::new();

    // We need to know the pages object ID ahead of time.
    // Layout: for each page: content_stream_obj, page_obj, fonts_obj (5 fonts)
    // Then: pages_obj, catalog_obj
    let fonts_per_page = 5; // Helvetica, Helvetica-Bold, Helvetica-Oblique, Helvetica-BoldOblique, Courier
    let pages_obj_id = (page_streams.len() as u32) * (2 + fonts_per_page) + 1;

    for page_stream in page_streams {
        let content_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", page_stream.len()),
            page_stream.clone(),
        );

        // Font IDs come right after content stream object
        let first_font_id = content_id + 1;

        let font_resources = format!(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
            FONT_HELVETICA
        );
        generator.add_object(font_resources);

        let font_bold_resources = format!(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
            FONT_HELVETICA_BOLD
        );
        generator.add_object(font_bold_resources);

        let font_italic_resources = format!(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
            FONT_HELVETICA_OBLIQUE
        );
        generator.add_object(font_italic_resources);

        let font_bold_italic_resources = format!(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
            FONT_HELVETICA_BOLD_OBLIQUE
        );
        generator.add_object(font_bold_italic_resources);

        let font_courier_resources = format!(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
            FONT_COURIER
        );
        generator.add_object(font_courier_resources);

        let page_dict = format!(
            "<< /Type /Page\n\
             /Parent {} 0 R\n\
             /MediaBox [0 0 {} {}]\n\
             /Contents {} 0 R\n\
             /Resources << /Font << \
                 /{} {} 0 R \
                 /{} {} 0 R \
                 /{} {} 0 R \
                 /{} {} 0 R \
                 /{} {} 0 R \
             >> >>\n\
             >>\n",
            pages_obj_id,
            layout.width,
            layout.height,
            content_id,
            FONT_HELVETICA, first_font_id,
            FONT_HELVETICA_BOLD, first_font_id + 1,
            FONT_HELVETICA_OBLIQUE, first_font_id + 2,
            FONT_HELVETICA_BOLD_OBLIQUE, first_font_id + 3,
            FONT_COURIER, first_font_id + 4
        );
        let page_id = generator.add_object(page_dict);
        page_ids.push(page_id);
    }

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let pages_dict = format!(
        "<< /Type /Pages\n\
         /Kids [{}]\n\
         /Count {}\n\
         >>\n",
        kids.join(" "),
        page_ids.len()
    );
    let actual_pages_id = generator.add_object(pages_dict);
    assert_eq!(actual_pages_id, pages_obj_id);

    let catalog_dict = format!(
        "<< /Type /Catalog\n\
         /Pages {} 0 R\n\
         >>\n",
        actual_pages_id
    );
    generator.add_object(catalog_dict);

    generator.generate()
}

/// Assemble final PDF from per-page content streams and write to file
fn assemble_pdf(filename: &str, page_streams: &[Vec<u8>], font: &str, layout: &PageLayout) -> Result<()> {
    let pdf_data = assemble_pdf_bytes(page_streams, font, layout);
    let mut file = File::create(filename)?;
    file.write_all(&pdf_data)?;
    Ok(())
}

/// Convert LaTeX-like math notation to readable text for PDF rendering.
/// Since Type1 fonts don't support full LaTeX glyph rendering, we convert
/// common math commands to their text/symbol equivalents.
fn render_math_text(expr: &str) -> String {
    let mut s = expr.to_string();

    // Greek letters
    let greek = [
        ("\\alpha", "\u{03B1}"), ("\\beta", "\u{03B2}"), ("\\gamma", "\u{03B3}"),
        ("\\delta", "\u{03B4}"), ("\\epsilon", "\u{03B5}"), ("\\zeta", "\u{03B6}"),
        ("\\eta", "\u{03B7}"), ("\\theta", "\u{03B8}"), ("\\iota", "\u{03B9}"),
        ("\\kappa", "\u{03BA}"), ("\\lambda", "\u{03BB}"), ("\\mu", "\u{03BC}"),
        ("\\nu", "\u{03BD}"), ("\\xi", "\u{03BE}"), ("\\pi", "\u{03C0}"),
        ("\\rho", "\u{03C1}"), ("\\sigma", "\u{03C3}"), ("\\tau", "\u{03C4}"),
        ("\\upsilon", "\u{03C5}"), ("\\phi", "\u{03C6}"), ("\\chi", "\u{03C7}"),
        ("\\psi", "\u{03C8}"), ("\\omega", "\u{03C9}"),
        ("\\Alpha", "A"), ("\\Beta", "B"), ("\\Gamma", "\u{0393}"),
        ("\\Delta", "\u{0394}"), ("\\Theta", "\u{0398}"), ("\\Lambda", "\u{039B}"),
        ("\\Xi", "\u{039E}"), ("\\Pi", "\u{03A0}"), ("\\Sigma", "\u{03A3}"),
        ("\\Phi", "\u{03A6}"), ("\\Psi", "\u{03A8}"), ("\\Omega", "\u{03A9}"),
    ];

    // Math operators and symbols
    let operators = [
        ("\\infty", "inf"), ("\\infinity", "inf"),
        ("\\pm", "+/-"), ("\\mp", "-/+"),
        ("\\times", "x"), ("\\cdot", "."),
        ("\\div", "/"), ("\\neq", "!="), ("\\ne", "!="),
        ("\\leq", "<="), ("\\le", "<="),
        ("\\geq", ">="), ("\\ge", ">="),
        ("\\approx", "~="), ("\\sim", "~"),
        ("\\equiv", "==="), ("\\propto", "~"),
        ("\\rightarrow", "->"), ("\\leftarrow", "<-"),
        ("\\Rightarrow", "=>"), ("\\Leftarrow", "<="),
        ("\\leftrightarrow", "<->"),
        ("\\forall", "for all"), ("\\exists", "there exists"),
        ("\\in", "in"), ("\\notin", "not in"),
        ("\\subset", "c="), ("\\supset", "=c"),
        ("\\cup", "U"), ("\\cap", "n"),
        ("\\emptyset", "{}"),
        ("\\nabla", "nabla"), ("\\partial", "d"),
        ("\\ldots", "..."), ("\\cdots", "..."), ("\\dots", "..."),
        ("\\quad", "  "), ("\\qquad", "    "),
        ("\\,", " "), ("\\;", " "), ("\\!", ""),
        ("\\left", ""), ("\\right", ""),
        ("\\big", ""), ("\\Big", ""), ("\\bigg", ""), ("\\Bigg", ""),
    ];

    // Apply Greek letter replacements (longer patterns first to avoid partial matches)
    for (cmd, replacement) in &greek {
        s = s.replace(cmd, replacement);
    }

    // Apply operator replacements
    for (cmd, replacement) in &operators {
        s = s.replace(cmd, replacement);
    }

    // Handle \frac{a}{b} -> (a)/(b)
    let frac_re = regex::Regex::new(r"\\frac\{([^}]*)\}\{([^}]*)\}").unwrap();
    while frac_re.is_match(&s) {
        s = frac_re.replace_all(&s, "($1)/($2)").to_string();
    }

    // Handle \sqrt{x} -> sqrt(x)
    let sqrt_re = regex::Regex::new(r"\\sqrt\{([^}]*)\}").unwrap();
    s = sqrt_re.replace_all(&s, "sqrt($1)").to_string();

    // Handle \sqrt[n]{x} -> n-root(x)
    let nroot_re = regex::Regex::new(r"\\sqrt\[([^\]]*)\]\{([^}]*)\}").unwrap();
    s = nroot_re.replace_all(&s, "$1-root($2)").to_string();

    // Handle \sum, \prod, \int with optional limits
    let sum_re = regex::Regex::new(r"\\sum_\{([^}]*)\}\^\{([^}]*)\}").unwrap();
    s = sum_re.replace_all(&s, "SUM($1 to $2)").to_string();
    s = s.replace("\\sum", "SUM");

    let prod_re = regex::Regex::new(r"\\prod_\{([^}]*)\}\^\{([^}]*)\}").unwrap();
    s = prod_re.replace_all(&s, "PROD($1 to $2)").to_string();
    s = s.replace("\\prod", "PROD");

    let int_re = regex::Regex::new(r"\\int_\{([^}]*)\}\^\{([^}]*)\}").unwrap();
    s = int_re.replace_all(&s, "INT($1 to $2)").to_string();
    s = s.replace("\\int", "INT");

    let lim_re = regex::Regex::new(r"\\lim_\{([^}]*)\}").unwrap();
    s = lim_re.replace_all(&s, "lim($1)").to_string();
    s = s.replace("\\lim", "lim");

    // Handle superscript ^{x} -> ^(x) and subscript _{x} -> _(x)
    let sup_re = regex::Regex::new(r"\^\{([^}]*)\}").unwrap();
    s = sup_re.replace_all(&s, "^($1)").to_string();

    let sub_re = regex::Regex::new(r"_\{([^}]*)\}").unwrap();
    s = sub_re.replace_all(&s, "_($1)").to_string();

    // Handle \text{...} -> ...
    let text_re = regex::Regex::new(r"\\text\{([^}]*)\}").unwrap();
    s = text_re.replace_all(&s, "$1").to_string();

    // Handle \mathbf{...}, \mathrm{...}, \mathit{...} -> content
    let mathfmt_re = regex::Regex::new(r"\\math[a-z]+\{([^}]*)\}").unwrap();
    s = mathfmt_re.replace_all(&s, "$1").to_string();

    // Handle \hat{x}, \bar{x}, \vec{x}, \tilde{x}
    let hat_re = regex::Regex::new(r"\\hat\{([^}]*)\}").unwrap();
    s = hat_re.replace_all(&s, "$1^").to_string();
    let bar_re = regex::Regex::new(r"\\bar\{([^}]*)\}").unwrap();
    s = bar_re.replace_all(&s, "$1_bar").to_string();
    let vec_re = regex::Regex::new(r"\\vec\{([^}]*)\}").unwrap();
    s = vec_re.replace_all(&s, "vec($1)").to_string();

    // Handle \log, \ln, \sin, \cos, \tan, \exp
    for func in &["log", "ln", "sin", "cos", "tan", "exp", "min", "max", "det", "dim"] {
        let cmd = format!("\\{}", func);
        s = s.replace(&cmd, func);
    }

    // Strip remaining braces
    s = s.replace('{', "").replace('}', "");

    // Clean up multiple spaces
    let multi_space = regex::Regex::new(r"  +").unwrap();
    s = multi_space.replace_all(&s, " ").to_string();

    s.trim().to_string()
}

fn escape_pdf_string(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

// --- Accessibility / Tagged PDF support ---

/// Accessibility options for PDF generation
#[derive(Debug, Clone)]
pub struct AccessibilityOptions {
    /// Enable tagged PDF (PDF/UA compliance)
    pub tagged_pdf: bool,
    /// Document language (e.g., "en-US", "en-GB")
    pub language: String,
    /// Document title for accessibility
    pub title: Option<String>,
}

impl Default for AccessibilityOptions {
    fn default() -> Self {
        Self {
            tagged_pdf: false,
            language: "en".to_string(),
            title: None,
        }
    }
}

impl AccessibilityOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tagged_pdf(mut self, tagged: bool) -> Self {
        self.tagged_pdf = tagged;
        self
    }

    pub fn with_language(mut self, lang: String) -> Self {
        self.language = lang;
        self
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }
}

/// Structure element types for tagged PDF
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureType {
    Document,
    Part,
    Art,
    Sect,
    Div,
    BlockQuote,
    Caption,
    TOC,
    TOCI,
    Index,
    NonStruct,
    Private,
    P,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    L,
    LI,
    Lbl,
    LBody,
    Table,
    TR,
    TH,
    TD,
    THead,
    TBody,
    TFoot,
    Span,
    Quote,
    Note,
    Reference,
    BibEntry,
    Code,
    Link,
    Figure,
    Formula,
}

impl StructureType {
    /// Get the PDF structure type name as per PDF 1.7 specification
    pub fn as_pdf_name(&self) -> &str {
        match self {
            Self::Document => "Document",
            Self::Part => "Part",
            Self::Art => "Art",
            Self::Sect => "Sect",
            Self::Div => "Div",
            Self::BlockQuote => "BlockQuote",
            Self::Caption => "Caption",
            Self::TOC => "TOC",
            Self::TOCI => "TOCI",
            Self::Index => "Index",
            Self::NonStruct => "NonStruct",
            Self::Private => "Private",
            Self::P => "P",
            Self::H1 => "H1",
            Self::H2 => "H2",
            Self::H3 => "H3",
            Self::H4 => "H4",
            Self::H5 => "H5",
            Self::H6 => "H6",
            Self::L => "L",
            Self::LI => "LI",
            Self::Lbl => "Lbl",
            Self::LBody => "LBody",
            Self::Table => "Table",
            Self::TR => "TR",
            Self::TH => "TH",
            Self::TD => "TD",
            Self::THead => "THead",
            Self::TBody => "TBody",
            Self::TFoot => "TFoot",
            Self::Span => "Span",
            Self::Quote => "Quote",
            Self::Note => "Note",
            Self::Reference => "Reference",
            Self::BibEntry => "BibEntry",
            Self::Code => "Code",
            Self::Link => "Link",
            Self::Figure => "Figure",
            Self::Formula => "Formula",
        }
    }
}

/// Structure element for tagged PDF
#[derive(Debug, Clone)]
pub struct StructureElement {
    pub struct_type: StructureType,
    pub alt_text: Option<String>,
    pub actual_text: Option<String>,
    pub children: Vec<StructureElement>,
    pub content_id: Option<u32>, // Reference to content object
}

impl StructureElement {
    pub fn new(struct_type: StructureType) -> Self {
        Self {
            struct_type,
            alt_text: None,
            actual_text: None,
            children: Vec::new(),
            content_id: None,
        }
    }

    pub fn with_alt_text(mut self, text: String) -> Self {
        self.alt_text = Some(text);
        self
    }

    pub fn with_actual_text(mut self, text: String) -> Self {
        self.actual_text = Some(text);
        self
    }

    pub fn with_children(mut self, children: Vec<StructureElement>) -> Self {
        self.children = children;
        self
    }

    pub fn add_child(&mut self, child: StructureElement) {
        self.children.push(child);
    }

    pub fn with_content_id(mut self, id: u32) -> Self {
        self.content_id = Some(id);
        self
    }

    /// Generate the structure element dictionary for PDF
    pub fn to_pdf_dict(&self, obj_id: u32) -> String {
        let mut dict = format!("<< /Type /StructElem /S /{}", self.struct_type.as_pdf_name());

        if let Some(ref alt) = self.alt_text {
            dict.push_str(&format!(" /Alt {}", escape_pdf_string(alt)));
        }

        if let Some(ref actual) = self.actual_text {
            dict.push_str(&format!(" /A {}", escape_pdf_string(actual)));
        }

        if let Some(ref content_id) = self.content_id {
            dict.push_str(&format!(" /K {} 0 R", content_id));
        } else if !self.children.is_empty() {
            let kid_refs: Vec<String> = self.children.iter()
                .enumerate()
                .map(|(i, _)| format!("{} 0 R", obj_id + 1 + i as u32))
                .collect();
            dict.push_str(&format!(" /K [{}]", kid_refs.join(" ")));
        } else {
            dict.push_str(" /K 0"); // No content
        }

        dict.push_str(" >>");
        dict
    }
}

/// Convert Element to StructureElement for accessibility
pub fn element_to_structure(element: &Element) -> StructureElement {
    match element {
        Element::Heading { level, text } => {
            let struct_type = match level {
                1 => StructureType::H1,
                2 => StructureType::H2,
                3 => StructureType::H3,
                4 => StructureType::H4,
                5 => StructureType::H5,
                _ => StructureType::H6,
            };
            StructureElement::new(struct_type)
                .with_actual_text(text.clone())
        }
        Element::Paragraph { text } => {
            StructureElement::new(StructureType::P)
                .with_actual_text(text.clone())
        }
        Element::RichParagraph { segments } => {
            let text = segments.iter().map(|s| match s {
                TextSegment::Plain(t) | TextSegment::Bold(t) | TextSegment::Italic(t) | TextSegment::BoldItalic(t) => t.clone(),
                TextSegment::Code(c) => format!("`{}`", c),
                TextSegment::Link { text, url } => format!("{} ({})", text, url),
            }).collect::<Vec<_>>().join("");
            StructureElement::new(StructureType::P)
                .with_actual_text(text)
        }
        Element::UnorderedListItem { text, .. } | Element::OrderedListItem { text, .. } | Element::TaskListItem { text, .. } => {
            StructureElement::new(StructureType::LI)
                .with_actual_text(text.clone())
        }
        Element::CodeBlock { code, .. } => {
            StructureElement::new(StructureType::Code)
                .with_actual_text(code.clone())
        }
        Element::BlockQuote { text, .. } => {
            StructureElement::new(StructureType::BlockQuote)
                .with_actual_text(text.clone())
        }
        Element::TableRow { .. } => {
            StructureElement::new(StructureType::TR)
        }
        Element::HorizontalRule => {
            StructureElement::new(StructureType::NonStruct)
        }
        Element::EmptyLine => {
            StructureElement::new(StructureType::NonStruct)
        }
        Element::Footnote { .. } => {
            StructureElement::new(StructureType::Note)
        }
        Element::DefinitionItem { .. } => {
            StructureElement::new(StructureType::Div)
        }
        Element::InlineCode { code } => {
            StructureElement::new(StructureType::Code)
                .with_actual_text(code.clone())
        }
        Element::Link { text, url } => {
            StructureElement::new(StructureType::Link)
                .with_actual_text(format!("{} ({})", text, url))
        }
        Element::Image { alt, .. } => {
            StructureElement::new(StructureType::Figure)
                .with_alt_text(alt.clone())
        }
        Element::StyledText { text, .. } => {
            StructureElement::new(StructureType::Span)
                .with_actual_text(text.clone())
        }
        Element::MathBlock { expression } => {
            StructureElement::new(StructureType::Formula)
                .with_actual_text(expression.clone())
        }
        Element::MathInline { expression } => {
            StructureElement::new(StructureType::Formula)
                .with_actual_text(expression.clone())
        }
        Element::PageBreak => {
            StructureElement::new(StructureType::NonStruct)
        }
    }
}

#[cfg(test)]
mod accessibility_tests {
    use super::*;

    #[test]
    fn test_accessibility_options_default() {
        let opts = AccessibilityOptions::default();
        assert!(!opts.tagged_pdf);
        assert_eq!(opts.language, "en");
        assert!(opts.title.is_none());
    }

    #[test]
    fn test_accessibility_options_builder() {
        let opts = AccessibilityOptions::new()
            .with_tagged_pdf(true)
            .with_language("en-US".to_string())
            .with_title("My Document".to_string());

        assert!(opts.tagged_pdf);
        assert_eq!(opts.language, "en-US");
        assert_eq!(opts.title, Some("My Document".to_string()));
    }

    #[test]
    fn test_structure_type_names() {
        assert_eq!(StructureType::Document.as_pdf_name(), "Document");
        assert_eq!(StructureType::P.as_pdf_name(), "P");
        assert_eq!(StructureType::H1.as_pdf_name(), "H1");
        assert_eq!(StructureType::Figure.as_pdf_name(), "Figure");
    }

    #[test]
    fn test_structure_element_builder() {
        let elem = StructureElement::new(StructureType::P)
            .with_alt_text("A paragraph".to_string())
            .with_actual_text("This is the actual text".to_string());

        assert_eq!(elem.struct_type, StructureType::P);
        assert_eq!(elem.alt_text, Some("A paragraph".to_string()));
        assert_eq!(elem.actual_text, Some("This is the actual text".to_string()));
    }

    #[test]
    fn test_structure_element_with_children() {
        let mut parent = StructureElement::new(StructureType::L);
        parent.add_child(StructureElement::new(StructureType::LI));
        parent.add_child(StructureElement::new(StructureType::LI));

        assert_eq!(parent.children.len(), 2);
    }

    #[test]
    fn test_element_to_structure_heading() {
        let elem = Element::Heading { level: 1, text: "Hello".into() };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::H1);
        assert_eq!(struct_elem.actual_text, Some("Hello".to_string()));
    }

    #[test]
    fn test_element_to_structure_paragraph() {
        let elem = Element::Paragraph { text: "Test paragraph".into() };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::P);
        assert_eq!(struct_elem.actual_text, Some("Test paragraph".to_string()));
    }

    #[test]
    fn test_element_to_structure_code() {
        let elem = Element::CodeBlock { language: "rust".into(), code: "fn main() {}".into() };
        let struct_elem = element_to_structure(&elem);

        assert_eq!(struct_elem.struct_type, StructureType::Code);
        assert_eq!(struct_elem.actual_text, Some("fn main() {}".to_string()));
    }
}
