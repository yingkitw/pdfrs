//! `generation` subcommand handlers.

use pdfrs::{
    comprehensive, elements, html, markdown, optimization, pdf_generator, pdf_ops, plugin,
};

use super::super::{build_plugin_registry, parse_optimization_profile};

pub(crate) fn cmd_generate_comprehensive(
    output: String,
    landscape: bool,
    linearize: bool,
    font_size: f32,
    columns: u8,
) {
    {
        let opts = comprehensive::ComprehensiveOptions::default()
            .with_landscape(landscape)
            .with_linearize(linearize)
            .with_font_size(font_size)
            .with_columns(columns);
        match comprehensive::write_bundled_comprehensive_pdf(&output, &opts) {
            Ok(()) => {
                let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
                println!(
                    "Wrote comprehensive PDF {} ({} bytes, linearize={}, columns={})",
                    output, size, linearize, columns
                );
            }
            Err(e) => eprintln!("Error generating comprehensive PDF: {}", e),
        }
    }
}

pub(crate) fn cmd_create(
    output: String,
    text: String,
    font: String,
    font_size: f32,
    landscape: bool,
    profile: String,
) {
    {
        let profile = parse_optimization_profile(&profile);
        let layout = if landscape {
            pdf_generator::PageLayout::landscape()
        } else {
            pdf_generator::PageLayout::portrait()
        };
        let elements: Vec<elements::Element> = text
            .lines()
            .map(|l| {
                if l.trim().is_empty() {
                    elements::Element::EmptyLine
                } else {
                    elements::Element::Paragraph {
                        text: l.to_string(),
                    }
                }
            })
            .collect();
        let generator = optimization::OptimizedPdfGenerator::new(profile)
            .with_font(&font)
            .with_font_size(font_size)
            .with_layout(layout);
        match generator.generate(&elements, &output) {
            Ok(_) => println!("PDF created successfully: {}", output),
            Err(e) => eprintln!("Error creating PDF: {}", e),
        }
    }
}

pub(crate) fn cmd_create_streaming(output: String, text: String, landscape: bool) {
    {
        let layout = if landscape {
            pdfrs::pdf_generator::PageLayout::landscape()
        } else {
            pdfrs::pdf_generator::PageLayout::portrait()
        };
        match pdfrs::streaming::StreamingPdfGenerator::new(&output, layout) {
            Ok(mut pdf_gen) => {
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        let _ = pdf_gen.add_paragraph("");
                    } else if let Some(text) = trimmed.strip_prefix("# ") {
                        let _ = pdf_gen.add_heading(text, 1);
                    } else if let Some(text) = trimmed.strip_prefix("## ") {
                        let _ = pdf_gen.add_heading(text, 2);
                    } else {
                        let _ = pdf_gen.add_paragraph(trimmed);
                    }
                }
                match pdf_gen.finish() {
                    Ok(_) => println!("Streaming PDF created successfully: {}", output),
                    Err(e) => eprintln!("Error finishing streaming PDF: {}", e),
                }
            }
            Err(e) => eprintln!("Error creating streaming PDF generator: {}", e),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_md_to_pdf(
    input: String,
    output: String,
    font: String,
    font_size: f32,
    landscape: bool,
    rtl: bool,
    columns: u8,
    plugins: String,
    profile: String,
) {
    {
        let profile = parse_optimization_profile(&profile);
        let layout = if landscape {
            pdf_generator::PageLayout::landscape()
        } else {
            pdf_generator::PageLayout::portrait()
        }
        .with_rtl(rtl)
        .with_columns(columns);
        let result = (|| -> anyhow::Result<()> {
            let content = std::fs::read_to_string(&input)?;
            let registry = build_plugin_registry(&plugins);
            let elements = if registry.has_parsers() || registry.has_generators() {
                plugin::parse_markdown_with_plugins(&content, &registry)
            } else {
                elements::parse_markdown(&content)
            };
            let mut generator = optimization::OptimizedPdfGenerator::new(profile)
                .with_font(&font)
                .with_font_size(font_size)
                .with_layout(layout);
            if let Some(parent) = std::path::Path::new(&input).parent() {
                generator = generator.with_image_base_dir(parent);
            }
            generator.generate(&elements, &output)?;
            Ok(())
        })();
        match result {
            Ok(_) => println!(
                "Successfully converted Markdown {} to PDF {}",
                input, output
            ),
            Err(e) => {
                eprintln!("Error converting Markdown to PDF: {}", e);
                std::process::exit(1);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_md_to_pdf_meta(
    input: String,
    output: String,
    title: Option<String>,
    author: Option<String>,
    subject: Option<String>,
    keywords: Option<String>,
    custom: Option<String>,
    font: String,
    font_size: f32,
    landscape: bool,
) {
    {
        let orientation = if landscape {
            pdf_generator::PageOrientation::Landscape
        } else {
            pdf_generator::PageOrientation::Portrait
        };
        let mut metadata = pdf_ops::PdfMetadata {
            title,
            author,
            subject,
            keywords,
            creator: Some("pdf-cli".into()),
            ..Default::default()
        };

        // Parse custom metadata fields (key=value pairs, comma-separated)
        if let Some(custom_fields) = custom {
            for field in custom_fields.split(',') {
                let parts: Vec<&str> = field.trim().split('=').collect();
                if parts.len() == 2 {
                    metadata
                        .add_custom_field(parts[0].trim().to_string(), parts[1].trim().to_string());
                } else {
                    eprintln!(
                        "Warning: Invalid custom field format: {}. Use key=value",
                        field
                    );
                }
            }
        }

        match pdf_ops::create_pdf_with_metadata(
            &input,
            &output,
            &font,
            font_size,
            orientation,
            &metadata,
        ) {
            Ok(_) => println!("Successfully created {} with metadata", output),
            Err(e) => eprintln!("Error creating PDF with metadata: {}", e),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_html_to_pdf(
    input: String,
    output: String,
    font: String,
    font_size: f32,
    landscape: bool,
    rtl: bool,
    columns: u8,
    profile: String,
) {
    {
        let profile = parse_optimization_profile(&profile);
        let layout = if landscape {
            pdf_generator::PageLayout::landscape()
        } else {
            pdf_generator::PageLayout::portrait()
        }
        .with_rtl(rtl)
        .with_columns(columns);
        let result = (|| -> anyhow::Result<()> {
            let content = std::fs::read_to_string(&input)?;
            let elements = html::parse_html(&content);
            let mut generator = optimization::OptimizedPdfGenerator::new(profile)
                .with_font(&font)
                .with_font_size(font_size)
                .with_layout(layout);
            if let Some(parent) = std::path::Path::new(&input).parent() {
                generator = generator.with_image_base_dir(parent);
            }
            generator.generate(&elements, &output)?;
            Ok(())
        })();
        match result {
            Ok(_) => println!("Successfully converted HTML {} to PDF {}", input, output),
            Err(e) => {
                eprintln!("Error converting HTML to PDF: {}", e);
                std::process::exit(1);
            }
        }
    }
}

pub(crate) fn cmd_watch_markdown(
    input: String,
    output: String,
    font: Option<String>,
    font_size: Option<f32>,
    orientation: Option<String>,
    interval: u64,
) {
    {
        let font = font.unwrap_or_else(|| "Helvetica".to_string());
        let font_size = font_size.unwrap_or(12.0);
        let orientation = match orientation.as_deref() {
            Some("landscape") => pdf_generator::PageOrientation::Landscape,
            _ => pdf_generator::PageOrientation::Portrait,
        };
        match markdown::watch_markdown_to_pdf(
            &input,
            &output,
            &font,
            font_size,
            orientation,
            Some(interval),
        ) {
            Ok(_) => {}
            Err(e) => eprintln!("Error watching markdown: {}", e),
        }
    }
}

pub(crate) fn cmd_create_form(
    output: String,
    text: String,
    fields: String,
    _font: String,
    _font_size: f32,
) {
    {
        // Read form fields from JSON file
        let fields_json = match std::fs::read_to_string(&fields) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Error reading form fields file: {}", e);
                return;
            }
        };

        let form_fields: Vec<pdf_ops::FormField> = match serde_json::from_str(&fields_json) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Error parsing form fields JSON: {}", e);
                eprintln!(
                    "Expected format: [{{\"name\":\"field1\",\"type\":\"Text\",\"x\":100,\"y\":700,\"width\":200,\"height\":20,\"default_value\":\"\",\"options\":[],\"required\":false}}]"
                );
                return;
            }
        };

        match pdf_ops::create_pdf_with_form_fields(&output, &text, &form_fields) {
            Ok(_) => println!(
                "Successfully created {} with {} form fields",
                output,
                form_fields.len()
            ),
            Err(e) => eprintln!("Error creating PDF with form fields: {}", e),
        }
    }
}
