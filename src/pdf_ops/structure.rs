//! Document structure detection (headings, sections) from PDF content streams.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A text fragment augmented with font information for structure detection.
#[derive(Debug, Clone, PartialEq)]
struct StyledTextFragment {
    text: String,
    x: f32,
    y: f32,
    font_size: f32,
    font_name: String,
}

/// A detected heading with its level and position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedHeading {
    pub level: u8,
    pub text: String,
    pub page_hint: Option<u32>,
}

/// A detected section of the document (content between headings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedSection {
    pub title: Option<String>,
    pub level: u8,
    pub content_lines: Vec<String>,
    pub has_table: bool,
}

/// The overall structure of a PDF document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentStructure {
    pub headings: Vec<DetectedHeading>,
    pub sections: Vec<DetectedSection>,
    pub estimated_page_count: u32,
    pub body_font_size: f32,
}

/// Detect the document structure (headings, sections, tables) of an existing PDF.
///
/// Analyzes text positioning and font sizes in PDF content streams to heuristically
/// identify headings, body text sections, and tables. Headings are detected when
/// a line's font size is significantly larger than the dominant (body) font size.
///
/// # Returns
///
/// A `DocumentStructure` containing headings, sections, and metadata.
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::pdf_ops::detect_document_structure;
///
/// let structure = detect_document_structure("report.pdf").unwrap();
/// for h in &structure.headings {
///     println!("H{}: {}", h.level, h.text);
/// }
/// ```
pub fn detect_document_structure(input_file: &str) -> Result<DocumentStructure> {
    use crate::pdf::{PdfDocument, PdfObject};

    let doc = PdfDocument::load_from_file(input_file)?;
    let mut all_fragments: Vec<StyledTextFragment> = Vec::new();

    let tj_re = regex::Regex::new(r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)\s*Tj").unwrap();
    let tj_hex_re = regex::Regex::new(r"<([0-9a-fA-F\s]+)>\s*Tj").unwrap();
    let td_re = regex::Regex::new(r"([\d.\-]+)\s+([\d.\-]+)\s+T[dD]").unwrap();
    let tm_re = regex::Regex::new(
        r"([\d.\-]+)\s+([\d.\-]+)\s+([\d.\-]+)\s+([\d.\-]+)\s+([\d.\-]+)\s+([\d.\-]+)\s+Tm",
    )
    .unwrap();
    let tf_re = regex::Regex::new(r"/(\S+)\s+([\d.\-]+)\s+Tf").unwrap();

    for obj in doc.objects.values() {
        if let PdfObject::Stream { data, .. } = obj {
            let processed_data =
                crate::compression::decompress_deflate(data).unwrap_or_else(|_| data.to_vec());
            let content = String::from_utf8_lossy(&processed_data);

            let mut current_x: f32 = 0.0;
            let mut current_y: f32 = 0.0;
            let mut current_font_size: f32 = 12.0;
            let mut current_font_name: String = String::new();
            let mut tm_scale: f32 = 1.0;

            for line in content.lines() {
                let line = line.trim();

                // Track font change: /FontName size Tf
                if let Some(caps) = tf_re.captures(line)
                    && let Ok(size) = caps[2].parse::<f32>()
                {
                    current_font_name = caps[1].to_string();
                    current_font_size = size;
                }

                // Track positioning
                if let Some(caps) = td_re.captures(line)
                    && let (Ok(x), Ok(y)) = (caps[1].parse::<f32>(), caps[2].parse::<f32>())
                {
                    current_x = x;
                    current_y = y;
                }
                if let Some(caps) = tm_re.captures(line)
                    && let (Ok(a), Ok(_d), Ok(x), Ok(y)) = (
                        caps[1].parse::<f32>(),
                        caps[4].parse::<f32>(),
                        caps[5].parse::<f32>(),
                        caps[6].parse::<f32>(),
                    )
                {
                    current_x = x;
                    current_y = y;
                    // Effective font scale from matrix (a = x-scale, d = y-scale)
                    tm_scale = a.abs();
                    // Also adjust font size by y-scale if it's meaningful
                    if let Ok(d) = caps[4].parse::<f32>()
                        && d.abs() > 0.01
                    {
                        tm_scale = d.abs();
                    }
                }

                // Extract text fragments
                for caps in tj_re.captures_iter(line) {
                    let extracted = &caps[1];
                    let unescaped = crate::pdf::unescape_pdf_string(extracted);
                    if !unescaped.trim().is_empty() {
                        all_fragments.push(StyledTextFragment {
                            text: unescaped.trim().to_string(),
                            x: current_x,
                            y: current_y,
                            font_size: current_font_size * tm_scale,
                            font_name: current_font_name.clone(),
                        });
                    }
                }

                for caps in tj_hex_re.captures_iter(line) {
                    let hex_str = caps[1].replace(char::is_whitespace, "");
                    let decoded = crate::pdf::decode_pdf_hex_string(&hex_str);
                    if !decoded.trim().is_empty() {
                        all_fragments.push(StyledTextFragment {
                            text: decoded.trim().to_string(),
                            x: current_x,
                            y: current_y,
                            font_size: current_font_size * tm_scale,
                            font_name: current_font_name.clone(),
                        });
                    }
                }
            }
        }
    }

    if all_fragments.is_empty() {
        return Ok(DocumentStructure {
            headings: Vec::new(),
            sections: Vec::new(),
            estimated_page_count: 1,
            body_font_size: 12.0,
        });
    }

    // Sort by Y descending (PDF: 0,0 bottom-left)
    all_fragments.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap());

    // Group into lines by Y position
    let y_tolerance = 3.0;
    let mut lines: Vec<Vec<StyledTextFragment>> = Vec::new();
    let mut current_line: Vec<StyledTextFragment> = Vec::new();
    let mut current_y = all_fragments[0].y;

    for frag in &all_fragments {
        let frag_y = frag.y;
        if (frag_y - current_y).abs() <= y_tolerance {
            current_line.push(frag.clone());
        } else {
            if !current_line.is_empty() {
                current_line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
                lines.push(current_line);
            }
            current_line = vec![frag.clone()];
            current_y = frag_y;
        }
    }
    if !current_line.is_empty() {
        current_line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        lines.push(current_line);
    }

    // Merge very close lines (same visual line)
    let mut merged_lines: Vec<Vec<StyledTextFragment>> = Vec::new();
    for line in lines {
        if let Some(last) = merged_lines.last_mut() {
            let last_y = last.iter().map(|f| f.y).sum::<f32>() / last.len() as f32;
            let this_y = line.iter().map(|f| f.y).sum::<f32>() / line.len() as f32;
            if (this_y - last_y).abs() <= 1.5 {
                last.extend(line);
                last.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
                continue;
            }
        }
        merged_lines.push(line);
    }

    // Compute body font size (most common non-zero size)
    let mut size_counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for line in &merged_lines {
        for frag in line {
            let size_key = (frag.font_size.round() as u32).max(1);
            *size_counts.entry(size_key).or_insert(0) += 1;
        }
    }
    let body_font_size = size_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(size, _)| size as f32)
        .unwrap_or(12.0);

    // Identify headings: font size >= 1.5x body, or bold font name, or short line with large font
    let mut headings: Vec<DetectedHeading> = Vec::new();
    let mut sections: Vec<DetectedSection> = Vec::new();
    let mut current_section_lines: Vec<String> = Vec::new();
    let mut current_section_level: u8 = 0;
    let mut current_section_title: Option<String> = None;

    for line in &merged_lines {
        let line_text: String = line
            .iter()
            .map(|f| &f.text as &str)
            .collect::<Vec<_>>()
            .join(" ");
        if line_text.trim().is_empty() {
            continue;
        }

        let avg_font_size =
            line.iter().map(|f| f.font_size).sum::<f32>() / line.len().max(1) as f32;
        let is_bold = line.iter().any(|f| {
            let name = f.font_name.to_lowercase();
            name.contains("bold") || name.contains("heavy") || name.contains("black")
        });
        let word_count = line_text.split_whitespace().count();

        // Heading heuristic
        let is_heading = if avg_font_size >= body_font_size * 2.0 {
            // Very large font → H1
            true
        } else if avg_font_size >= body_font_size * 1.5 {
            // Large font → H2
            true
        } else if is_bold && word_count <= 10 && avg_font_size >= body_font_size * 1.1 {
            // Bold and short → could be heading
            true
        } else {
            false
        };

        if is_heading {
            // Save previous section
            if !current_section_lines.is_empty() || current_section_title.is_some() {
                sections.push(DetectedSection {
                    title: current_section_title.clone(),
                    level: current_section_level,
                    content_lines: current_section_lines.clone(),
                    has_table: false, // detected later
                });
            }

            let level = if avg_font_size >= body_font_size * 2.0 {
                1
            } else if avg_font_size >= body_font_size * 1.5 {
                2
            } else {
                3
            };

            headings.push(DetectedHeading {
                level,
                text: line_text.trim().to_string(),
                page_hint: None,
            });

            current_section_title = Some(line_text.trim().to_string());
            current_section_level = level;
            current_section_lines = Vec::new();
        } else {
            current_section_lines.push(line_text.trim().to_string());
        }
    }

    // Push final section
    if !current_section_lines.is_empty() || current_section_title.is_some() {
        sections.push(DetectedSection {
            title: current_section_title,
            level: current_section_level,
            content_lines: current_section_lines,
            has_table: false,
        });
    }

    // Estimate page count from Y range (A4 = 842 pts height)
    let y_min = all_fragments
        .iter()
        .map(|f| f.y)
        .fold(f32::INFINITY, f32::min);
    let y_max = all_fragments
        .iter()
        .map(|f| f.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let estimated_pages = ((y_max - y_min) / 800.0).ceil().max(1.0) as u32;

    Ok(DocumentStructure {
        headings,
        sections,
        estimated_page_count: estimated_pages,
        body_font_size,
    })
}
