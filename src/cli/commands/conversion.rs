//! `conversion` subcommand handlers.

use pdfrs::{pdf, pdf_ops, pdf_to_md};

pub(crate) fn cmd_pdf_to_md(input: String, output: String) {
    match std::fs::read(&input) {
        Ok(pdf_bytes) => match pdf_to_md::pdf_to_markdown_bytes(&pdf_bytes) {
            Ok(md) => {
                if let Err(e) = std::fs::write(&output, md) {
                    eprintln!("Error writing Markdown file: {}", e);
                } else {
                    println!(
                        "Successfully converted PDF {} to Markdown {}",
                        input, output
                    );
                }
            }
            Err(e) => {
                // Fall back to the legacy plain-text extractor.
                eprintln!(
                    "Structured conversion failed ({}); falling back to plain text",
                    e
                );
                match pdf::extract_text(&input) {
                    Ok(text) => {
                        if let Err(e) = std::fs::write(&output, text) {
                            eprintln!("Error writing Markdown file: {}", e);
                        } else {
                            println!(
                                "Successfully converted PDF {} to Markdown {} (plain text fallback)",
                                input, output
                            );
                        }
                    }
                    Err(e) => eprintln!("Error extracting text from PDF: {}", e),
                }
            }
        },
        Err(e) => eprintln!("Error reading PDF: {}", e),
    }
}

pub(crate) fn cmd_extract(input: String) {
    match pdf::extract_text(&input) {
        Ok(text) => println!("Extracted text:\n{}", text),
        Err(e) => eprintln!("Error extracting text: {}", e),
    }
}

pub(crate) fn cmd_detect_structure(input: String) {
    match pdf_ops::detect_document_structure(&input) {
        Ok(structure) => {
            if structure.headings.is_empty() {
                println!("No headings detected in {}", input);
                println!("Estimated pages: {}", structure.estimated_page_count);
                println!("Body font size: {}pt", structure.body_font_size);
            } else {
                println!(
                    "Detected {} heading(s) in {} (est. {} pages):",
                    structure.headings.len(),
                    input,
                    structure.estimated_page_count
                );
                for h in &structure.headings {
                    let indent = "  ".repeat(h.level as usize);
                    println!("{}{} {}", indent, "#".repeat(h.level as usize), h.text);
                }
                println!("\nSections:");
                for s in &structure.sections {
                    if let Some(ref title) = s.title {
                        println!("  - {} ({} content lines)", title, s.content_lines.len());
                    } else {
                        println!("  - [untitled] ({} content lines)", s.content_lines.len());
                    }
                }
            }
        }
        Err(e) => eprintln!("Error detecting structure: {}", e),
    }
}

pub(crate) fn cmd_extract_tables(input: String, output: String) {
    {
        match pdf_ops::extract_tables_from_pdf(&input) {
            Ok(tables) => {
                if tables.is_empty() {
                    println!("No tables found in {}", input);
                } else {
                    let mut csv = String::new();
                    for (i, table_csv) in tables.iter().enumerate() {
                        if i > 0 {
                            csv.push_str("\n---\n");
                        }
                        csv.push_str(table_csv);
                    }
                    match std::fs::write(&output, csv) {
                        Ok(_) => println!("Extracted {} table(s) to {}", tables.len(), output),
                        Err(e) => eprintln!("Error writing CSV: {}", e),
                    }
                }
            }
            Err(e) => eprintln!("Error extracting tables: {}", e),
        }
    }
}

pub(crate) fn cmd_extract_images(input: String, output: String) {
    {
        match pdf_ops::extract_images_from_pdf(&input, &output) {
            Ok(files) => {
                if files.is_empty() {
                    println!("No embedded images found in {}", input);
                } else {
                    println!(
                        "Extracted {} image(s) from {} to {}",
                        files.len(),
                        input,
                        output
                    );
                    for f in &files {
                        println!("  - {}", f);
                    }
                }
            }
            Err(e) => eprintln!("Error extracting images: {}", e),
        }
    }
}

pub(crate) fn cmd_detect_form_fields(input: String) {
    match pdf_ops::detect_form_fields(&input) {
        Ok(fields) => {
            if fields.is_empty() {
                println!("No form fields found in {}", input);
            } else {
                println!("Found {} form field(s) in {}:", fields.len(), input);
                for f in &fields {
                    let value_str = f.value.as_deref().unwrap_or("(empty)");
                    let req_str = if f.required { " [required]" } else { "" };
                    println!(
                        "  - {} ({}) = {}{}",
                        f.name, f.field_type, value_str, req_str
                    );
                    if !f.options.is_empty() {
                        println!("    options: {}", f.options.join(", "));
                    }
                }
            }
        }
        Err(e) => eprintln!("Error detecting form fields: {}", e),
    }
}

pub(crate) fn cmd_fill_form_fields(input: String, output: String, values: String) {
    {
        let field_values: std::collections::HashMap<String, String> =
            match serde_json::from_str(&values) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error parsing field values JSON: {}", e);
                    eprintln!(
                        "Expected format: {{\"fieldName\":\"value\",\"otherField\":\"otherValue\"}}"
                    );
                    return;
                }
            };

        match pdf_ops::fill_form_fields(&input, &output, &field_values) {
            Ok(_) => println!("Successfully filled form fields in {}", output),
            Err(e) => eprintln!("Error filling form fields: {}", e),
        }
    }
}
