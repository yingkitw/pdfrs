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
}

// Use the library instead of declaring modules
use pdf_rs::{compression, elements, image, markdown, pdf, pdf_generator, pdf_ops, security};

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
        } => {
            let orientation = if landscape {
                pdf_generator::PageOrientation::Landscape
            } else {
                pdf_generator::PageOrientation::Portrait
            };
            match markdown::markdown_to_pdf_full(&input, &output, &font, font_size, orientation) {
            Ok(_) => println!(
                "Successfully converted Markdown {} to PDF {}",
                input, output
            ),
            Err(e) => eprintln!("Error converting Markdown to PDF: {}", e),
        }},
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
        } => {
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
            match pdf_generator::create_pdf_from_elements_with_layout(&output, &elements, &font, font_size, layout) {
                Ok(_) => println!("PDF created successfully: {}", output),
                Err(e) => eprintln!("Error creating PDF: {}", e),
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
            let refs: Vec<&str> = inputs.iter().map(|s| s.as_str()).collect();
            match pdf_ops::merge_pdfs(&refs, &output) {
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
            font,
            font_size,
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
                "aes-128" => security::EncryptionAlgorithm::Aes_128,
                "aes-256" => security::EncryptionAlgorithm::Aes_256,
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
    }
}
