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
        #[arg(long, help = "Font family", default_value = "Helvetica")]
        font: String,
        #[arg(long, help = "Font size", default_value = "12")]
        font_size: f32,
        #[arg(long, help = "Use landscape orientation")]
        landscape: bool,
    },
}

mod compression;
mod elements;
mod image;
mod markdown;
mod pdf;
mod pdf_generator;
mod pdf_ops;

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
            font,
            font_size,
            landscape,
        } => {
            let orientation = if landscape {
                pdf_generator::PageOrientation::Landscape
            } else {
                pdf_generator::PageOrientation::Portrait
            };
            let metadata = pdf_ops::PdfMetadata {
                title,
                author,
                subject,
                keywords,
                creator: Some("pdf-cli".into()),
            };
            match pdf_ops::create_pdf_with_metadata(&input, &output, &font, font_size, orientation, &metadata) {
                Ok(_) => println!("Successfully created {} with metadata", output),
                Err(e) => eprintln!("Error creating PDF with metadata: {}", e),
            }
        }
    }
}
