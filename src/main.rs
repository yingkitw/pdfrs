use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pdf-cli")]
#[command(about = "A CLI tool to read/write PDFs and convert to/from markdown")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Convert PDF to Markdown")]
    PdfToMd {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(help = "Output Markdown file")]
        output: String,
    },
    #[command(about = "Convert Markdown to PDF")]
    MdToPdf {
        #[arg(help = "Input Markdown file")]
        input: String,
        #[arg(help = "Output PDF file")]
        output: String,
        #[arg(long, help = "Font family", default_value = "Helvetica")]
        font: String,
        #[arg(long, help = "Font size", default_value = "12")]
        font_size: f32,
        #[arg(long, help = "Use landscape orientation")]
        landscape: bool,
        #[arg(long, help = "Optimization profile (web, print, archive, ebook)", default_value = "archive")]
        profile: String,
    },
    #[command(about = "Extract text from PDF")]
    Extract {
        #[arg(help = "Input PDF file")]
        input: String,
    },
    #[command(about = "Create a new PDF")]
    Create {
        #[arg(help = "Output PDF file")]
        output: String,
        #[arg(help = "Text content for the PDF")]
        text: String,
        #[arg(long, help = "Font family", default_value = "Helvetica")]
        font: String,
        #[arg(long, help = "Font size", default_value = "12")]
        font_size: f32,
        #[arg(long, help = "Use landscape orientation")]
        landscape: bool,
        #[arg(long, help = "Optimization profile (web, print, archive, ebook)", default_value = "archive")]
        profile: String,
    },
    #[command(about = "Create a new PDF with streaming (memory-efficient for large docs)")]
    CreateStreaming {
        #[arg(help = "Output PDF file")]
        output: String,
        #[arg(help = "Text content for the PDF")]
        text: String,
        #[arg(long, help = "Use landscape orientation")]
        landscape: bool,
    },
    #[command(about = "Add image to PDF")]
    AddImage {
        #[arg(help = "PDF file to modify")]
        pdf_file: String,
        #[arg(help = "Image file to add")]
        image_file: String,
        #[arg(long, help = "X position", default_value = "100")]
        x: f32,
        #[arg(long, help = "Y position", default_value = "100")]
        y: f32,
        #[arg(long, help = "Width", default_value = "200")]
        width: f32,
        #[arg(long, help = "Height", default_value = "200")]
        height: f32,
    },
    #[command(about = "Merge multiple PDFs into one")]
    Merge {
        #[arg(help = "Input PDF files", num_args = 2..)]
        inputs: Vec<String>,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
    },
    #[command(about = "Split PDF by extracting page range")]
    Split {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(long, help = "Start page (1-indexed)", default_value = "1")]
        start: usize,
        #[arg(long, help = "End page (1-indexed, inclusive)")]
        end: usize,
    },
    #[command(about = "Add text watermark to PDF")]
    Watermark {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(long, help = "Watermark text")]
        text: String,
        #[arg(long, help = "Font size for watermark", default_value = "48")]
        size: f32,
        #[arg(long, help = "Opacity (0.0-1.0)", default_value = "0.3")]
        opacity: f32,
    },
    #[command(about = "Reorder pages in a PDF")]
    Reorder {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(long, help = "Page order (comma-separated, 1-indexed)")]
        pages: String,
    },
    #[command(about = "Rotate all pages in a PDF")]
    Rotate {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(long, help = "Rotation angle (0, 90, 180, 270)")]
        angle: u32,
    },
    #[command(about = "Set PDF metadata and convert from Markdown")]
    MdToPdfMeta {
        #[arg(help = "Input Markdown file")]
        input: String,
        #[arg(help = "Output PDF file")]
        output: String,
        #[arg(long, help = "Document title")]
        title: Option<String>,
        #[arg(long, help = "Document author")]
        author: Option<String>,
        #[arg(long, help = "Document subject")]
        subject: Option<String>,
        #[arg(long, help = "Document keywords")]
        keywords: Option<String>,
        #[arg(long, help = "Custom metadata fields (key=value pairs, comma-separated)")]
        custom: Option<String>,
        #[arg(long, help = "Font family", default_value = "Helvetica")]
        font: String,
        #[arg(long, help = "Font size", default_value = "12")]
        font_size: f32,
        #[arg(long, help = "Use landscape orientation")]
        landscape: bool,
    },
    #[command(about = "Create PDF with form fields")]
    CreateForm {
        #[arg(help = "Output PDF file")]
        output: String,
        #[arg(help = "Text content for the PDF")]
        text: String,
        #[arg(long, help = "Form fields JSON file")]
        fields: String,
        #[arg(long, help = "Font family", default_value = "Helvetica")]
        font: String,
        #[arg(long, help = "Font size", default_value = "12")]
        font_size: f32,
    },
    #[command(about = "Detect form fields in an existing PDF")]
    DetectFormFields {
        #[arg(help = "Input PDF file")]
        input: String,
    },
    #[command(about = "Fill form fields in a PDF with new values")]
    FillFormFields {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(long, help = "Field values as JSON object {\"fieldName\":\"value\"}")]
        values: String,
    },
    #[command(about = "Detect document structure (headings, sections) in a PDF")]
    DetectStructure {
        #[arg(help = "Input PDF file")]
        input: String,
    },
    #[command(about = "Optimize a PDF (recompress streams, reduce file size)")]
    OptimizePdf {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(short, long, help = "Optimization profile (web, print, archive, ebook)", default_value = "web")]
        profile: String,
    },
    #[command(about = "Overlay an image onto all pages of a PDF")]
    OverlayImage {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(long, help = "Image file to overlay")]
        image: String,
        #[arg(long, help = "X position", default_value = "100")]
        x: f32,
        #[arg(long, help = "Y position", default_value = "100")]
        y: f32,
        #[arg(long, help = "Width", default_value = "200")]
        width: f32,
        #[arg(long, help = "Height", default_value = "200")]
        height: f32,
        #[arg(long, help = "Opacity (0.0-1.0)", default_value = "1.0")]
        opacity: f32,
    },
    #[command(about = "Add watermark to PDF (text or image)")]
    WatermarkAdvanced {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(long, help = "Text watermark")]
        text: Option<String>,
        #[arg(long, help = "Image watermark file")]
        image: Option<String>,
        #[arg(long, help = "Opacity (0.0-1.0)", default_value = "0.3")]
        opacity: f32,
        #[arg(long, help = "Position (center, topleft, topright, bottomleft, bottomright, diagonal)", default_value = "diagonal")]
        position: String,
    },
    #[command(about = "Extract tables from a PDF to CSV")]
    ExtractTables {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output CSV file")]
        output: String,
    },
    #[command(about = "Extract embedded images from a PDF")]
    ExtractImages {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output directory for extracted images", default_value = "extracted_images")]
        output: String,
    },
    #[command(about = "Add a digital signature to a PDF")]
    Sign {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(help = "Output signed PDF file")]
        output: String,
        #[arg(long, help = "Signer name", default_value = "")]
        signer: String,
        #[arg(long, help = "Reason for signing")]
        reason: Option<String>,
        #[arg(long, help = "Signing location")]
        location: Option<String>,
        #[arg(long, help = "Contact information")]
        contact: Option<String>,
    },
    #[command(about = "Verify digital signatures in a PDF")]
    VerifySignature {
        #[arg(help = "Input PDF file")]
        input: String,
    },
    #[command(about = "Add password protection and permissions to PDF")]
    Protect {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(long, help = "User password (required to open document)")]
        user_password: Option<String>,
        #[arg(long, help = "Owner password (controls permissions)")]
        owner_password: Option<String>,
        #[arg(long, help = "Encryption algorithm (rc4-40, rc4-128, aes-128, aes-256)", default_value = "rc4-128")]
        algorithm: String,
        #[arg(long, help = "Allow printing")]
        allow_print: bool,
        #[arg(long, help = "Allow copying content")]
        allow_copy: bool,
        #[arg(long, help = "Allow modifying document")]
        allow_modify: bool,
        #[arg(long, help = "Allow annotations")]
        allow_annotate: bool,
        #[arg(long, help = "Allow filling forms")]
        allow_fill_forms: bool,
        #[arg(long, help = "Allow extracting content for accessibility")]
        allow_extract: bool,
        #[arg(long, help = "Allow assembling (insert, rotate, delete pages)")]
        allow_assemble: bool,
        #[arg(long, help = "Allow high-quality printing")]
        allow_print_high_quality: bool,
        #[arg(long, help = "Read-only (no modifications)")]
        read_only: bool,
    },
    #[command(about = "Validate PDF structural integrity")]
    Validate {
        #[arg(help = "Input PDF file")]
        input: String,
    },
    #[command(about = "Validate PDF/A-1b compliance")]
    ValidatePdfa {
        #[arg(help = "Input PDF file")]
        input: String,
    },
}

// Use the library instead of declaring modules
use pdfrs::{elements, image, optimization, parallel, pdf, pdf_generator, pdf_ops, security};

fn parse_optimization_profile(s: &str) -> optimization::OptimizationProfile {
    match s.to_lowercase().as_str() {
        "web" => optimization::OptimizationProfile::web(),
        "print" => optimization::OptimizationProfile::print(),
        "archive" => optimization::OptimizationProfile::archive(),
        "ebook" => optimization::OptimizationProfile::ebook(),
        _ => optimization::OptimizationProfile::archive(),
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::PdfToMd { input, output } => match pdf::extract_text(&input) {
            Ok(text) => {
                if let Err(e) = std::fs::write(&output, text) {
                    eprintln!("Error writing Markdown file: {}", e);
                } else {
                    println!(
                        "Successfully converted PDF {} to Markdown {}",
                        input, output
                    );
                }
            }
            Err(e) => eprintln!("Error extracting text from PDF: {}", e),
        },
        Commands::MdToPdf {
            input,
            output,
            font,
            font_size,
            landscape,
            profile,
        } => {
            let profile = parse_optimization_profile(&profile);
            let layout = if landscape {
                pdf_generator::PageLayout::landscape()
            } else {
                pdf_generator::PageLayout::portrait()
            };
            let result = (|| -> anyhow::Result<()> {
                let content = std::fs::read_to_string(&input)?;
                let elements = elements::parse_markdown(&content);
                let generator = optimization::OptimizedPdfGenerator::new(profile)
                    .with_font(&font)
                    .with_font_size(font_size)
                    .with_layout(layout);
                generator.generate(&elements, &output)
            })();
            match result {
                Ok(_) => println!(
                    "Successfully converted Markdown {} to PDF {}",
                    input, output
                ),
                Err(e) => eprintln!("Error converting Markdown to PDF: {}", e),
            }
        },
        Commands::Extract { input } => match pdf::extract_text(&input) {
            Ok(text) => println!("Extracted text:\n{}", text),
            Err(e) => eprintln!("Error extracting text: {}", e),
        },
        Commands::Create {
            output,
            text,
            font,
            font_size,
            landscape,
            profile,
        } => {
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
                        elements::Element::Paragraph { text: l.to_string() }
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
        },
        Commands::CreateStreaming {
            output,
            text,
            landscape,
        } => {
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
                        } else if trimmed.starts_with("# ") {
                            let _ = pdf_gen.add_heading(&trimmed[2..], 1);
                        } else if trimmed.starts_with("## ") {
                            let _ = pdf_gen.add_heading(&trimmed[3..], 2);
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
        },
        Commands::AddImage {
            pdf_file,
            image_file,
            x,
            y,
            width,
            height,
        } => match image::add_image_to_pdf(&pdf_file, &image_file, x, y, width, height) {
            Ok(_) => println!(
                "Successfully added image {} to PDF {}",
                image_file, pdf_file
            ),
            Err(e) => eprintln!("Error adding image: {}", e),
        },
        Commands::Merge { inputs, output } => {
            match parallel::merge_pdfs_parallel(&inputs, output.clone()) {
                Ok(_) => println!("Successfully merged into {}", output),
                Err(e) => eprintln!("Error merging PDFs: {}", e),
            }
        }
        Commands::Split { input, output, start, end } => {
            match pdf_ops::split_pdf(&input, &output, start, end) {
                Ok(_) => println!("Successfully split {} into {}", input, output),
                Err(e) => eprintln!("Error splitting PDF: {}", e),
            }
        }
        Commands::Watermark { input, output, text, size, opacity } => {
            match pdf_ops::watermark_pdf(&input, &output, &text, size, opacity) {
                Ok(_) => println!("Successfully watermarked into {}", output),
                Err(e) => eprintln!("Error adding watermark: {}", e),
            }
        }
        Commands::Reorder { input, output, pages } => {
            let order: Result<Vec<usize>, _> = pages.split(',').map(|s| s.trim().parse::<usize>()).collect();
            match order {
                Ok(page_order) => {
                    match pdf_ops::reorder_pages(&input, &output, &page_order) {
                        Ok(_) => println!("Successfully reordered into {}", output),
                        Err(e) => eprintln!("Error reordering pages: {}", e),
                    }
                }
                Err(e) => eprintln!("Invalid page order format: {}. Use comma-separated numbers like 3,1,2", e),
            }
        }
        Commands::Rotate { input, output, angle } => {
            match pdf_ops::rotate_pdf(&input, &output, angle) {
                Ok(_) => println!("Successfully rotated {} into {}", input, output),
                Err(e) => eprintln!("Error rotating PDF: {}", e),
            }
        }
        Commands::MdToPdfMeta {
            input,
            output,
            title,
            author,
            subject,
            keywords,
            custom,
            font,
            font_size,
            landscape,
        } => {
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
                        metadata.add_custom_field(parts[0].trim().to_string(), parts[1].trim().to_string());
                    } else {
                        eprintln!("Warning: Invalid custom field format: {}. Use key=value", field);
                    }
                }
            }

            match pdf_ops::create_pdf_with_metadata(&input, &output, &font, font_size, orientation, &metadata) {
                Ok(_) => println!("Successfully created {} with metadata", output),
                Err(e) => eprintln!("Error creating PDF with metadata: {}", e),
            }
        }
        Commands::CreateForm {
            output,
            text,
            fields,
            font: _font,
            font_size: _font_size,
        } => {
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
                    eprintln!("Expected format: [{{\"name\":\"field1\",\"type\":\"Text\",\"x\":100,\"y\":700,\"width\":200,\"height\":20,\"default_value\":\"\",\"options\":[],\"required\":false}}]");
                    return;
                }
            };

            match pdf_ops::create_pdf_with_form_fields(&output, &text, &form_fields) {
                Ok(_) => println!("Successfully created {} with {} form fields", output, form_fields.len()),
                Err(e) => eprintln!("Error creating PDF with form fields: {}", e),
            }
        }
        Commands::DetectFormFields { input } => {
            match pdf_ops::detect_form_fields(&input) {
                Ok(fields) => {
                    if fields.is_empty() {
                        println!("No form fields found in {}", input);
                    } else {
                        println!("Found {} form field(s) in {}:", fields.len(), input);
                        for f in &fields {
                            let value_str = f.value.as_deref().unwrap_or("(empty)");
                            let req_str = if f.required { " [required]" } else { "" };
                            println!("  - {} ({}) = {}{}", f.name, f.field_type, value_str, req_str);
                            if !f.options.is_empty() {
                                println!("    options: {}", f.options.join(", "));
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Error detecting form fields: {}", e),
            }
        }
        Commands::FillFormFields { input, output, values } => {
            let field_values: std::collections::HashMap<String, String> = match serde_json::from_str(&values) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error parsing field values JSON: {}", e);
                    eprintln!("Expected format: {{\"fieldName\":\"value\",\"otherField\":\"otherValue\"}}");
                    return;
                }
            };

            match pdf_ops::fill_form_fields(&input, &output, &field_values) {
                Ok(_) => println!("Successfully filled form fields in {}", output),
                Err(e) => eprintln!("Error filling form fields: {}", e),
            }
        }
        Commands::DetectStructure { input } => {
            match pdf_ops::detect_document_structure(&input) {
                Ok(structure) => {
                    if structure.headings.is_empty() {
                        println!("No headings detected in {}", input);
                        println!("Estimated pages: {}", structure.estimated_page_count);
                        println!("Body font size: {}pt", structure.body_font_size);
                    } else {
                        println!("Detected {} heading(s) in {} (est. {} pages):", structure.headings.len(), input, structure.estimated_page_count);
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
        Commands::OptimizePdf { input, output, profile } => {
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
                        if in_size > 0 { (out_size as f64 / in_size as f64) * 100.0 } else { 0.0 }
                    );
                    println!("Profile: {:?} | compression: {:?}", profile, settings.compression_level);
                }
                Err(e) => eprintln!("Error optimizing PDF: {}", e),
            }
        }
        Commands::OverlayImage {
            input,
            output,
            image,
            x,
            y,
            width,
            height,
            opacity,
        } => {
            match pdf_ops::overlay_image_on_pdf(&input, &output, &image, x, y, width, height, opacity) {
                Ok(_) => println!("Successfully overlaid image on {}", output),
                Err(e) => eprintln!("Error overlaying image: {}", e),
            }
        }
        Commands::WatermarkAdvanced {
            input,
            output,
            text,
            image,
            opacity,
            position,
        } => {
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
                    eprintln!("Error: Invalid position '{}'. Valid options: center, topleft, topright, bottomleft, bottomright, diagonal", position);
                    return;
                }
            };

            match pdf_ops::watermark_pdf_advanced(&input, &output, watermark_content, opacity, watermark_position) {
                Ok(_) => println!("Successfully added watermark to {}", output),
                Err(e) => eprintln!("Error adding watermark: {}", e),
            }
        }
        Commands::ExtractTables { input, output } => {
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
        Commands::ExtractImages { input, output } => {
            match pdf_ops::extract_images_from_pdf(&input, &output) {
                Ok(files) => {
                    if files.is_empty() {
                        println!("No embedded images found in {}", input);
                    } else {
                        println!("Extracted {} image(s) from {} to {}", files.len(), input, output);
                        for f in &files {
                            println!("  - {}", f);
                        }
                    }
                }
                Err(e) => eprintln!("Error extracting images: {}", e),
            }
        }
        Commands::Sign {
            input,
            output,
            signer,
            reason,
            location,
            contact,
        } => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let date = format!("{}0000+0000", now);
            let sig = security::DigitalSignature::new(&signer)
                .with_date(date);
            let sig = if let Some(r) = reason {
                sig.with_reason(r)
            } else {
                sig
            };
            let sig = if let Some(l) = location {
                sig.with_location(l)
            } else {
                sig
            };
            let sig = if let Some(c) = contact {
                sig.with_contact_info(c)
            } else {
                sig
            };
            match pdf_ops::sign_pdf(&input, &output, &sig) {
                Ok(_) => println!("Successfully signed {} -> {}", input, output),
                Err(e) => eprintln!("Error signing PDF: {}", e),
            }
        }
        Commands::VerifySignature { input } => {
            match pdf_ops::verify_pdf_signature(&input) {
                Ok(sigs) => {
                    if sigs.is_empty() {
                        println!("No digital signatures found in {}", input);
                    } else {
                        println!("Found {} signature(s) in {}:", sigs.len(), input);
                        for (i, sig) in sigs.iter().enumerate() {
                            println!("  Signature #{}:", i + 1);
                            println!("    Signer: {}", sig.signer_name);
                            if let Some(ref reason) = sig.reason {
                                println!("    Reason: {}", reason);
                            }
                            if let Some(ref location) = sig.location {
                                println!("    Location: {}", location);
                            }
                            if let Some(ref date) = sig.date {
                                println!("    Date: {}", date);
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Error verifying signatures: {}", e),
            }
        }
        Commands::Protect {
            input,
            output,
            user_password,
            owner_password,
            algorithm,
            allow_print,
            allow_copy,
            allow_modify,
            allow_annotate,
            allow_fill_forms,
            allow_extract,
            allow_assemble,
            allow_print_high_quality,
            read_only,
        } => {
            // Check if at least one password is provided
            if user_password.is_none() && owner_password.is_none() {
                eprintln!("Error: At least one of --user-password or --owner-password must be specified");
                return;
            }

            // Parse encryption algorithm
            let encryption_algo = match algorithm.to_lowercase().as_str() {
                "rc4-40" => security::EncryptionAlgorithm::Rc4_40,
                "rc4-128" => security::EncryptionAlgorithm::Rc4_128,
                "aes-128" => security::EncryptionAlgorithm::Aes128,
                "aes-256" => security::EncryptionAlgorithm::Aes256,
                _ => {
                    eprintln!("Error: Invalid algorithm '{}'. Valid options: rc4-40, rc4-128, aes-128, aes-256", algorithm);
                    return;
                }
            };

            // Create permissions
            let permissions = if read_only {
                security::PdfPermissions::read_only()
            } else {
                security::PdfPermissions {
                    print: allow_print,
                    copy: allow_copy,
                    modify: allow_modify,
                    annotate: allow_annotate,
                    fill_forms: allow_fill_forms,
                    extract: allow_extract,
                    assemble: allow_assemble,
                    print_high_quality: allow_print_high_quality,
                }
            };

            // Create security settings
            let mut sec = security::PdfSecurity::new()
                .with_encryption(encryption_algo)
                .with_permissions(permissions);

            if let Some(user_pwd) = user_password {
                sec = sec.with_user_password(user_pwd);
            }
            if let Some(owner_pwd) = owner_password {
                sec = sec.with_owner_password(owner_pwd);
            }

            // Validate security settings
            if let Err(e) = sec.validate() {
                eprintln!("Error: {}", e);
                return;
            }

            match pdf_ops::protect_pdf(&input, &output, &sec) {
                Ok(_) => println!("Successfully applied protection to {}", output),
                Err(e) => eprintln!("Error protecting PDF: {}", e),
            }
        }
        Commands::Validate { input } => {
            match pdf::validate_pdf(&input) {
                Ok(result) => {
                    println!("Validation result for {}:", input);
                    println!("  Valid: {}", result.valid);
                    println!("  Pages: {}", result.page_count);
                    println!("  Objects: {}", result.object_count);
                    if !result.errors.is_empty() {
                        println!("  Errors:");
                        for e in &result.errors {
                            println!("    - {}", e);
                        }
                    }
                    if !result.warnings.is_empty() {
                        println!("  Warnings:");
                        for w in &result.warnings {
                            println!("    - {}", w);
                        }
                    }
                }
                Err(e) => eprintln!("Error validating PDF: {}", e),
            }
        }
        Commands::ValidatePdfa { input } => {
            match pdf::validate_pdf_a(&input) {
                Ok(result) => {
                    println!("PDF/A validation result for {}:", input);
                    println!("  Level: {}", result.level);
                    println!("  Compliant: {}", result.compliant);
                    println!("  Embedded fonts: {}", result.embedded_fonts);
                    println!("  Has XMP metadata: {}", result.has_xmp);
                    println!("  Has encryption: {}", result.has_encryption);
                    if !result.errors.is_empty() {
                        println!("  Errors:");
                        for e in &result.errors {
                            println!("    - {}", e);
                        }
                    }
                    if !result.warnings.is_empty() {
                        println!("  Warnings:");
                        for w in &result.warnings {
                            println!("    - {}", w);
                        }
                    }
                }
                Err(e) => eprintln!("Error validating PDF/A: {}", e),
            }
        }
    }
}
