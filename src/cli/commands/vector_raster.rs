//! `vector_raster` subcommand handlers.

use pdfrs::{pdf_generator, raster, vector};

pub(crate) fn cmd_draw_vector(output: String, landscape: bool) {
    {
        let layout = if landscape {
            pdf_generator::PageLayout::landscape()
        } else {
            pdf_generator::PageLayout::portrait()
        };
        match vector::demo_canvas().write_pdf(&output, layout) {
            Ok(_) => println!("Wrote vector graphics demo PDF: {}", output),
            Err(e) => eprintln!("Error writing vector PDF: {}", e),
        }
    }
}

pub(crate) fn cmd_draw_svg(
    output: String,
    path: Option<String>,
    file: Option<String>,
    landscape: bool,
    line_width: f32,
    fill: bool,
) {
    {
        let layout = if landscape {
            pdf_generator::PageLayout::landscape()
        } else {
            pdf_generator::PageLayout::portrait()
        };
        let stroke = Some(pdf_generator::Color::black());
        let fill_color = if fill {
            Some(pdf_generator::Color::rgb(0.85, 0.9, 1.0))
        } else {
            None
        };
        let result = match (&path, &file) {
            (Some(d), _) => {
                match vector::svg_path_to_pdf_bytes(d, layout, stroke, fill_color, line_width) {
                    Ok(bytes) => std::fs::write(&output, bytes).map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                }
            }
            (None, Some(svg_file)) => vector::svg_document_file_to_pdf(svg_file, &output, layout)
                .map_err(|e| e.to_string()),
            (None, None) => Err("Provide --path \"M...\" or --file icon.svg".to_string()),
        };
        match result {
            Ok(_) => println!("Wrote SVG path PDF: {}", output),
            Err(e) => eprintln!("Error rendering SVG path: {}", e),
        }
    }
}

pub(crate) fn cmd_draw_svg_file(input: String, output: String, landscape: bool) {
    {
        let layout = if landscape {
            pdf_generator::PageLayout::landscape()
        } else {
            pdf_generator::PageLayout::portrait()
        };
        match vector::svg_document_file_to_pdf(&input, &output, layout) {
            Ok(_) => println!("Wrote SVG document PDF: {}", output),
            Err(e) => eprintln!("Error rendering SVG document: {}", e),
        }
    }
}

pub(crate) fn cmd_rasterize_pdf(input: String, output: String, page: Option<usize>, dpi: u32) {
    {
        let bytes = match std::fs::read(&input) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error reading PDF: {}", e);
                return;
            }
        };
        if let Some(idx) = page {
            match raster::rasterize_page(&bytes, idx, dpi) {
                Ok(page_image) => match page_image.write_png(&output) {
                    Ok(_) => println!(
                        "Wrote {} (page {}, {}×{} px)",
                        output, idx, page_image.width, page_image.height
                    ),
                    Err(e) => eprintln!("Error writing PNG: {}", e),
                },
                Err(e) => eprintln!("Error rasterizing page: {}", e),
            }
        } else {
            match raster::rasterize_all(&bytes, dpi) {
                Ok(pages) => {
                    let out_path = std::path::Path::new(&output);
                    if pages.len() == 1 {
                        match pages[0].write_png(&output) {
                            Ok(_) => println!(
                                "Wrote {} ({}×{} px)",
                                output, pages[0].width, pages[0].height
                            ),
                            Err(e) => eprintln!("Error writing PNG: {}", e),
                        }
                    } else {
                        std::fs::create_dir_all(out_path).ok();
                        for (i, p) in pages.iter().enumerate() {
                            let file = out_path.join(format!("page-{:04}.png", i + 1));
                            match p.write_png(file.to_str().unwrap_or_default()) {
                                Ok(_) => {}
                                Err(e) => eprintln!("Error writing page {}: {}", i + 1, e),
                            }
                        }
                        println!("Wrote {} page(s) to {}", pages.len(), output);
                    }
                }
                Err(e) => eprintln!("Error rasterizing PDF: {}", e),
            }
        }
    }
}
