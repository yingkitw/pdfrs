//! Table extraction from PDF content streams via position heuristics.

use anyhow::Result;

/// A single text fragment with its position in a PDF content stream
#[derive(Debug, Clone, PartialEq)]
struct TextFragment {
    text: String,
    x: f32,
    y: f32,
}

/// Extract tables from a PDF and return them as CSV strings.
///
/// This function analyzes the text positioning in PDF content streams
/// to heuristically detect tables. It groups text fragments by Y position
/// into rows, then sorts by X position within each row to form columns.
///
/// # Returns
/// A vector of CSV strings, one per detected table.
pub fn extract_tables_from_pdf(input_file: &str) -> Result<Vec<String>> {
    use crate::pdf::{PdfDocument, PdfObject};

    let doc = PdfDocument::load_from_file(input_file)?;
    let mut all_fragments: Vec<TextFragment> = Vec::new();

    // Regex patterns for text extraction with positioning
    let tj_re = regex::Regex::new(r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)\s*Tj").unwrap();
    let tj_hex_re = regex::Regex::new(r"<([0-9a-fA-F\s]+)>\s*Tj").unwrap();
    let td_re = regex::Regex::new(r"([\d.\-]+)\s+([\d.\-]+)\s+T[dD]").unwrap();
    let tm_re = regex::Regex::new(
        r"[\d.\-]+\s+[\d.\-]+\s+[\d.\-]+\s+[\d.\-]+\s+([\d.\-]+)\s+([\d.\-]+)\s+Tm",
    )
    .unwrap();

    for obj in doc.objects.values() {
        if let PdfObject::Stream { data, .. } = obj {
            let processed_data =
                crate::compression::decompress_deflate(data).unwrap_or_else(|_| data.to_vec());
            let content = String::from_utf8_lossy(&processed_data);

            let mut current_x: f32 = 0.0;
            let mut current_y: f32 = 0.0;

            for line in content.lines() {
                let line = line.trim();

                // Track positioning
                if let Some(caps) = td_re.captures(line)
                    && let (Ok(x), Ok(y)) = (caps[1].parse::<f32>(), caps[2].parse::<f32>())
                {
                    current_x = x;
                    current_y = y;
                }
                if let Some(caps) = tm_re.captures(line)
                    && let (Ok(x), Ok(y)) = (caps[1].parse::<f32>(), caps[2].parse::<f32>())
                {
                    current_x = x;
                    current_y = y;
                }

                // Extract text fragments with current position
                for caps in tj_re.captures_iter(line) {
                    let extracted = &caps[1];
                    let unescaped = crate::pdf::unescape_pdf_string(extracted);
                    if !unescaped.trim().is_empty() {
                        all_fragments.push(TextFragment {
                            text: unescaped.trim().to_string(),
                            x: current_x,
                            y: current_y,
                        });
                    }
                }

                for caps in tj_hex_re.captures_iter(line) {
                    let hex_str = caps[1].replace(char::is_whitespace, "");
                    let decoded = crate::pdf::decode_pdf_hex_string(&hex_str);
                    if !decoded.trim().is_empty() {
                        all_fragments.push(TextFragment {
                            text: decoded.trim().to_string(),
                            x: current_x,
                            y: current_y,
                        });
                    }
                }
            }
        }
    }

    if all_fragments.is_empty() {
        return Ok(Vec::new());
    }

    // Sort by Y descending (PDF coordinates: 0,0 is bottom-left)
    all_fragments.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap());

    // Group into rows by Y position (within tolerance)
    let y_tolerance = 3.0; // points
    let mut rows: Vec<Vec<TextFragment>> = Vec::new();
    let mut current_row: Vec<TextFragment> = Vec::new();
    let mut current_y = all_fragments[0].y;

    for frag in all_fragments {
        let frag_y = frag.y;
        if (frag_y - current_y).abs() <= y_tolerance {
            current_row.push(frag);
        } else {
            if !current_row.is_empty() {
                current_row.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
                rows.push(current_row);
            }
            current_row = vec![frag];
            current_y = frag_y;
        }
    }
    if !current_row.is_empty() {
        current_row.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        rows.push(current_row);
    }

    // Merge rows with very similar Y positions (same line, slight variations)
    let mut merged_rows: Vec<Vec<TextFragment>> = Vec::new();
    for row in rows {
        if let Some(last) = merged_rows.last_mut() {
            let last_y = last.iter().map(|f| f.y).sum::<f32>() / last.len() as f32;
            let row_y = row.iter().map(|f| f.y).sum::<f32>() / row.len() as f32;
            if (last_y - row_y).abs() <= y_tolerance {
                last.extend(row);
                last.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
                continue;
            }
        }
        merged_rows.push(row);
    }

    // Detect tables: find consecutive rows with similar structure
    let mut tables: Vec<Vec<Vec<String>>> = Vec::new();
    let mut current_table: Vec<Vec<String>> = Vec::new();
    let x_tolerance = 8.0; // points for column grouping

    for row in &merged_rows {
        let cells = group_row_into_cells(row, x_tolerance);
        if cells.len() >= 2 {
            current_table.push(cells);
        } else if !current_table.is_empty() {
            if current_table.len() >= 2 {
                tables.push(current_table);
            }
            current_table = Vec::new();
        }
    }
    if !current_table.is_empty() && current_table.len() >= 2 {
        tables.push(current_table);
    }

    // Convert tables to CSV
    let mut csv_outputs = Vec::new();
    for table in tables {
        let mut csv = String::new();
        for row in table {
            let escaped: Vec<String> = row.iter().map(|cell| escape_csv_field(cell)).collect();
            csv.push_str(&escaped.join(","));
            csv.push('\n');
        }
        csv_outputs.push(csv);
    }

    Ok(csv_outputs)
}

fn group_row_into_cells(row: &[TextFragment], x_tolerance: f32) -> Vec<String> {
    if row.is_empty() {
        return Vec::new();
    }

    let mut cells: Vec<Vec<String>> = Vec::new();
    let mut current_cell: Vec<String> = Vec::new();
    let mut last_x = row[0].x;

    for frag in row {
        if (frag.x - last_x).abs() > x_tolerance && !current_cell.is_empty() {
            cells.push(current_cell);
            current_cell = Vec::new();
        }
        current_cell.push(frag.text.clone());
        last_x = frag.x;
    }
    if !current_cell.is_empty() {
        cells.push(current_cell);
    }

    cells.into_iter().map(|parts| parts.join(" ")).collect()
}

fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        let escaped = field.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    } else {
        field.to_string()
    }
}
