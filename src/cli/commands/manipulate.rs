//! `manipulate` subcommand handlers.

use pdfrs::parallel;
use pdfrs::{image, incremental, linearize, optimization, pdf, pdf_ops};

use super::super::parse_optimization_profile;

pub(crate) fn cmd_add_image(
    pdf_file: String,
    image_file: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
    match image::add_image_to_pdf(&pdf_file, &image_file, x, y, width, height) {
        Ok(_) => println!(
            "Successfully added image {} to PDF {}",
            image_file, pdf_file
        ),
        Err(e) => eprintln!("Error adding image: {}", e),
    }
}

pub(crate) fn cmd_filter_image(
    input: String,
    output: String,
    filters: Vec<String>,
    width: f32,
    height: f32,
) {
    {
        if filters.is_empty() {
            eprintln!("Error: at least one --filter is required");
            return;
        }
        let parsed: Result<Vec<_>, _> = filters
            .iter()
            .map(|f| image::ImageFilter::parse(f))
            .collect();
        match parsed {
            Ok(filter_list) => {
                match image::create_filtered_image_pdf(&input, &output, &filter_list, width, height)
                {
                    Ok(_) => println!(
                        "Wrote filtered image PDF {} ({} filter(s))",
                        output,
                        filter_list.len()
                    ),
                    Err(e) => eprintln!("Error filtering image: {}", e),
                }
            }
            Err(e) => eprintln!("Error parsing filters: {}", e),
        }
    }
}

pub(crate) fn cmd_merge(inputs: Vec<String>, output: String) {
    {
        #[cfg(feature = "parallel")]
        {
            match parallel::merge_pdfs_parallel(&inputs, output.clone()) {
                Ok(_) => println!("Successfully merged into {}", output),
                Err(e) => eprintln!("Error merging PDFs: {}", e),
            }
        }
        #[cfg(not(feature = "parallel"))]
        {
            let input_refs: Vec<&str> = inputs.iter().map(|s| s.as_str()).collect();
            match pdf_ops::merge_pdfs(&input_refs, &output) {
                Ok(_) => println!("Successfully merged into {}", output),
                Err(e) => eprintln!("Error merging PDFs: {}", e),
            }
        }
    }
}

pub(crate) fn cmd_split(input: String, output: String, start: usize, end: usize) {
    match pdf_ops::split_pdf(&input, &output, start, end) {
        Ok(_) => println!("Successfully split {} into {}", input, output),
        Err(e) => eprintln!("Error splitting PDF: {}", e),
    }
}

pub(crate) fn cmd_watermark(input: String, output: String, text: String, size: f32, opacity: f32) {
    match pdf_ops::watermark_pdf(&input, &output, &text, size, opacity) {
        Ok(_) => println!("Successfully watermarked into {}", output),
        Err(e) => eprintln!("Error adding watermark: {}", e),
    }
}

pub(crate) fn cmd_watermark_advanced(
    input: String,
    output: String,
    text: Option<String>,
    image: Option<String>,
    opacity: f32,
    position: String,
) {
    {
        // Determine watermark content
        let watermark_content = if let Some(text_str) = text {
            pdf_ops::WatermarkContent::Text(text_str)
        } else if let Some(img_path) = image {
            pdf_ops::WatermarkContent::Image(img_path)
        } else {
            eprintln!("Error: Either --text or --image must be specified");
            return;
        };

        // Parse position
        let watermark_position = match position.to_lowercase().as_str() {
            "center" => pdf_ops::WatermarkPosition::Center,
            "topleft" => pdf_ops::WatermarkPosition::TopLeft,
            "topright" => pdf_ops::WatermarkPosition::TopRight,
            "bottomleft" => pdf_ops::WatermarkPosition::BottomLeft,
            "bottomright" => pdf_ops::WatermarkPosition::BottomRight,
            "diagonal" => pdf_ops::WatermarkPosition::Diagonal,
            _ => {
                eprintln!(
                    "Error: Invalid position '{}'. Valid options: center, topleft, topright, bottomleft, bottomright, diagonal",
                    position
                );
                return;
            }
        };

        match pdf_ops::watermark_pdf_advanced(
            &input,
            &output,
            watermark_content,
            opacity,
            watermark_position,
        ) {
            Ok(_) => println!("Successfully added watermark to {}", output),
            Err(e) => eprintln!("Error adding watermark: {}", e),
        }
    }
}

pub(crate) fn cmd_reorder(input: String, output: String, pages: String) {
    {
        let order: Result<Vec<usize>, _> = pages
            .split(',')
            .map(|s| s.trim().parse::<usize>())
            .collect();
        match order {
            Ok(page_order) => match pdf_ops::reorder_pages(&input, &output, &page_order) {
                Ok(_) => println!("Successfully reordered into {}", output),
                Err(e) => eprintln!("Error reordering pages: {}", e),
            },
            Err(e) => eprintln!(
                "Invalid page order format: {}. Use comma-separated numbers like 3,1,2",
                e
            ),
        }
    }
}

pub(crate) fn cmd_rotate(input: String, output: String, angle: u32) {
    match pdf_ops::rotate_pdf(&input, &output, angle) {
        Ok(_) => println!("Successfully rotated {} into {}", input, output),
        Err(e) => eprintln!("Error rotating PDF: {}", e),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_overlay_image(
    input: String,
    output: String,
    image: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    opacity: f32,
) {
    {
        match pdf_ops::overlay_image_on_pdf(&input, &output, &image, x, y, width, height, opacity) {
            Ok(_) => println!("Successfully overlaid image on {}", output),
            Err(e) => eprintln!("Error overlaying image: {}", e),
        }
    }
}

pub(crate) fn cmd_optimize_pdf(input: String, output: String, profile: String) {
    {
        let profile = parse_optimization_profile(&profile);
        let settings = profile.settings();
        match optimization::optimize_pdf_file(&input, &output, profile) {
            Ok(_) => {
                let in_size = std::fs::metadata(&input).map(|m| m.len()).unwrap_or(0);
                let out_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
                println!(
                    "Successfully optimized PDF: {} -> {} ({:.1}% of original)",
                    input,
                    output,
                    if in_size > 0 {
                        (out_size as f64 / in_size as f64) * 100.0
                    } else {
                        0.0
                    }
                );
                println!(
                    "Profile: {:?} | compression: {:?} | linearized: {}",
                    profile, settings.compression_level, settings.linearize
                );
                if settings.linearize
                    && std::fs::read(&output)
                        .ok()
                        .is_some_and(|b| linearize::is_linearized(&b))
                {
                    println!("Fast Web View: enabled (/Linearized)");
                }
            }
            Err(e) => eprintln!("Error optimizing PDF: {}", e),
        }
    }
}

pub(crate) fn cmd_linearize_pdf(input: String, output: String) {
    {
        match linearize::linearize_pdf_file(&input, &output) {
            Ok(_) => {
                println!("Linearized PDF written to {} (Fast Web View)", output);
            }
            Err(e) => eprintln!("Error linearizing PDF: {}", e),
        }
    }
}

pub(crate) fn cmd_incremental_update(
    input: String,
    output: String,
    title: Option<String>,
    author: Option<String>,
    note: Option<String>,
) {
    match std::fs::read(&input) {
        Ok(bytes) => {
            let mut updated = bytes;
            let mut did = false;
            if title.is_some() || author.is_some() {
                match incremental::incremental_set_info(
                    &updated,
                    title.as_deref(),
                    author.as_deref(),
                ) {
                    Ok(u) => {
                        updated = u;
                        did = true;
                    }
                    Err(e) => {
                        eprintln!("Error updating info: {}", e);
                        return;
                    }
                }
            }
            if let Some(ref n) = note {
                match incremental::incremental_add_text_annotation(
                    &updated, n, 72.0, 720.0, 24.0, 24.0,
                ) {
                    Ok(u) => {
                        updated = u;
                        did = true;
                    }
                    Err(e) => {
                        eprintln!("Error adding note: {}", e);
                        return;
                    }
                }
            }
            if !did {
                eprintln!("Provide --title/--author and/or --note");
                return;
            }
            match std::fs::write(&output, &updated) {
                Ok(_) => println!(
                    "Wrote incremental update to {} ({} -> {} bytes)",
                    output,
                    std::fs::metadata(&input).map(|m| m.len()).unwrap_or(0),
                    updated.len()
                ),
                Err(e) => eprintln!("Error writing output: {}", e),
            }
        }
        Err(e) => eprintln!("Error reading {}: {}", input, e),
    }
}

pub(crate) fn cmd_attach_file(input: String, output: String, file: String, name: Option<String>) {
    {
        let attachment_name = name.as_deref().unwrap_or_else(|| {
            std::path::Path::new(&file)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&file)
        });

        match pdf::PdfDocument::load_from_file(&input) {
            Ok(mut doc) => {
                let data = match std::fs::read(&file) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        eprintln!("Error reading attachment file: {}", e);
                        return;
                    }
                };
                match doc.embed_file(attachment_name, &data) {
                    Ok(_) => match std::fs::write(&output, doc.to_bytes()) {
                        Ok(_) => {
                            println!("Attached '{}' to {} as '{}'", file, input, attachment_name)
                        }
                        Err(e) => eprintln!("Error writing output PDF: {}", e),
                    },
                    Err(e) => eprintln!("Error embedding file: {}", e),
                }
            }
            Err(e) => eprintln!("Error loading PDF: {}", e),
        }
    }
}

pub(crate) fn cmd_create_portfolio(output: String, files: Vec<String>, title: Option<String>) {
    {
        if files.is_empty() {
            eprintln!("Error: no files provided for portfolio");
            return;
        }
        let file_tuples: Vec<(String, String)> = files
            .iter()
            .map(|f| {
                let desc = std::path::Path::new(f)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(f)
                    .to_string();
                (f.clone(), desc)
            })
            .collect();
        match pdf_ops::create_portfolio_pdf(&output, &file_tuples, title.as_deref()) {
            Ok(_) => println!(
                "Created portfolio PDF with {} file(s): {}",
                files.len(),
                output
            ),
            Err(e) => eprintln!("Error creating portfolio: {}", e),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_embed3d(
    output: String,
    model: String,
    label: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    activate_on_open: bool,
) {
    match std::fs::read(&model) {
        Ok(u3d_data) => {
            let annot = pdf_ops::ThreeDAnnotation {
                x,
                y,
                width,
                height,
                contents: label.clone(),
                activate_on_open,
            };
            match pdf_ops::create_pdf_with_3d_annotation(&output, &label, &u3d_data, &annot) {
                Ok(_) => println!("Created 3D PDF {} from {}", output, model),
                Err(e) => eprintln!("Error creating 3D PDF: {}", e),
            }
        }
        Err(e) => eprintln!("Error reading U3D model {}: {}", model, e),
    }
}
