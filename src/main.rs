use clap::{Parser, Subcommand};

mod cli_repl;

#[derive(Parser)]
#[command(name = "pdf-cli")]
#[command(about = "A CLI tool to read/write PDFs and convert to/from markdown")]
struct Cli {
    /// UI language for validation/error messages (en, es, de, fr, zh, he, ar).
    /// Falls back to PDFRS_LANG / LANG when omitted.
    #[arg(long, global = true)]
    lang: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Generate the bundled comprehensive capability PDF")]
    GenerateComprehensive {
        #[arg(short, long, help = "Output PDF file", default_value = "comprehensive.pdf")]
        output: String,
        #[arg(long, help = "Use landscape orientation")]
        landscape: bool,
        #[arg(long, help = "Also linearize for Fast Web View")]
        linearize: bool,
        #[arg(long, help = "Font size", default_value = "11")]
        font_size: f32,
        #[arg(long, help = "Number of text columns (1-4)", default_value = "1")]
        columns: u8,
    },
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
        #[arg(long, help = "Right-to-left layout (Hebrew/Arabic)")]
        rtl: bool,
        #[arg(long, help = "Number of text columns (1-4)", default_value = "1")]
        columns: u8,
        #[arg(long, help = "Enable plugins (comma-separated: callouts)", default_value = "")]
        plugins: String,
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
    #[command(about = "Apply image filters and write a one-page PDF (BMP/PNG)")]
    FilterImage {
        #[arg(help = "Input image file (BMP or PNG)")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(
            long = "filter",
            help = "Filter to apply (repeatable): grayscale, invert, sepia, brightness:N, contrast:F"
        )]
        filters: Vec<String>,
        #[arg(long, help = "Max display width", default_value = "500")]
        width: f32,
        #[arg(long, help = "Max display height", default_value = "700")]
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
    #[command(about = "Linearize a PDF for Fast Web View (progressive loading)")]
    LinearizePdf {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
    },
    #[command(about = "Append an incremental update (metadata) without rewriting the PDF body")]
    IncrementalUpdate {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(long, help = "Set document title via incremental /Info")]
        title: Option<String>,
        #[arg(long, help = "Set document author via incremental /Info")]
        author: Option<String>,
        #[arg(long, help = "Append a text annotation note (incremental)")]
        note: Option<String>,
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
        #[arg(long, help = "Path to signing certificate PEM file")]
        certificate: Option<String>,
        #[arg(long, help = "Certificate id in store (alternative to --certificate)")]
        cert_id: Option<String>,
        #[arg(long, help = "Certificate store directory", default_value = "certs")]
        cert_store: String,
    },
    #[command(about = "Import an X.509 certificate into the certificate store")]
    ImportCertificate {
        #[arg(help = "Certificate id")]
        id: String,
        #[arg(help = "Path to PEM certificate file")]
        file: String,
        #[arg(long, help = "Subject distinguished name override")]
        subject: Option<String>,
        #[arg(long, help = "Certificate store directory", default_value = "certs")]
        store: String,
    },
    #[command(about = "List certificates in the certificate store")]
    ListCertificates {
        #[arg(long, help = "Certificate store directory", default_value = "certs")]
        store: String,
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
    #[command(about = "Validate PDF/A-3b compliance")]
    ValidatePdfa3 {
        #[arg(help = "Input PDF file")]
        input: String,
    },
    #[command(about = "Validate PDF/UA (accessibility) compliance")]
    ValidatePdfua {
        #[arg(help = "Input PDF file")]
        input: String,
    },
    #[command(about = "Check screen reader compliance (PDF/UA + text extraction)")]
    CheckScreenReader {
        #[arg(help = "Input PDF file")]
        input: String,
    },
    #[command(about = "Compare two PDFs structurally and report differences")]
    DiffPdfs {
        #[arg(help = "Old PDF file")]
        old: String,
        #[arg(help = "New PDF file")]
        new: String,
    },
    #[command(about = "Sanitize a PDF by removing dangerous content (JS, launch actions, etc.)")]
    SanitizePdf {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output sanitized PDF file")]
        output: String,
    },
    #[command(about = "Sandbox JavaScript actions in a PDF (detect, strip, and report)")]
    SandboxPdf {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output sandboxed PDF file")]
        output: String,
    },
    #[command(about = "Watch a markdown file and regenerate PDF on changes")]
    WatchMarkdown {
        #[arg(help = "Input markdown file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(short, long, help = "Font name")]
        font: Option<String>,
        #[arg(short, long, help = "Font size")]
        font_size: Option<f32>,
        #[arg(short, long, help = "Page orientation")]
        orientation: Option<String>,
        #[arg(short, long, help = "Poll interval in milliseconds", default_value = "1000")]
        interval: u64,
    },
    #[command(about = "Interactive REPL for PDF manipulation")]
    Repl,
    #[command(about = "Create a PDF portfolio (collection) from multiple files")]
    CreatePortfolio {
        #[arg(short, long, help = "Output portfolio PDF file")]
        output: String,
        #[arg(help = "Files to include in the portfolio")]
        files: Vec<String>,
        #[arg(short, long, help = "Portfolio title")]
        title: Option<String>,
    },
    #[command(about = "Create a PDF with vector graphics (demo shapes)")]
    DrawVector {
        #[arg(help = "Output PDF file")]
        output: String,
        #[arg(long, help = "Use landscape page")]
        landscape: bool,
    },
    #[command(about = "Render an SVG path (d=) or SVG file into a PDF")]
    DrawSvg {
        #[arg(help = "Output PDF file")]
        output: String,
        #[arg(long, help = "SVG path d attribute string")]
        path: Option<String>,
        #[arg(long, help = "SVG file containing a <path d=\"...\">")]
        file: Option<String>,
        #[arg(long, help = "Use landscape page")]
        landscape: bool,
        #[arg(long, help = "Stroke line width", default_value = "1.5")]
        line_width: f32,
        #[arg(long, help = "Fill the path")]
        fill: bool,
    },
    #[command(about = "Attach an external file to a PDF")]
    AttachFile {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(help = "File to attach")]
        file: String,
        #[arg(short, long, help = "Attachment name in PDF (defaults to file basename)")]
        name: Option<String>,
    },
    #[command(about = "Create a PDF with an embedded U3D 3D annotation")]
    Embed3d {
        #[arg(help = "Output PDF file")]
        output: String,
        #[arg(help = "U3D model file")]
        model: String,
        #[arg(long, help = "Page label text", default_value = "3D Model")]
        label: String,
        #[arg(long, help = "Annotation X (points)", default_value = "72")]
        x: f32,
        #[arg(long, help = "Annotation Y (points)", default_value = "200")]
        y: f32,
        #[arg(long, help = "Annotation width", default_value = "400")]
        width: f32,
        #[arg(long, help = "Annotation height", default_value = "300")]
        height: f32,
        #[arg(long, help = "Activate 3D view when the page opens")]
        activate_on_open: bool,
    },
    #[command(about = "Rasterize PDF pages to PNG (pure Rust, no external deps)")]
    RasterizePdf {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output PNG file (single page) or directory (all pages)")]
        output: String,
        #[arg(long, help = "Page index (0-based); omit to rasterize every page")]
        page: Option<usize>,
        #[arg(long, help = "Resolution in DPI", default_value = "96")]
        dpi: u32,
    },
    #[command(about = "Search text inside a PDF and report page + bounding box per hit")]
    SearchPdf {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(help = "Search query")]
        query: String,
        #[arg(long, help = "Case-insensitive matching")]
        case_insensitive: bool,
        #[arg(long, help = "Output JSON file with hits (optional)")]
        json: Option<String>,
    },
    #[command(about = "Redact rectangular regions of a PDF (rewrites content streams)")]
    RedactPdf {
        #[arg(help = "Input PDF file")]
        input: String,
        #[arg(short, long, help = "Output redacted PDF file")]
        output: String,
        #[arg(
            long,
            help = "Redaction region: page,x,y,w,h (repeat for multiple regions)"
        )]
        region: Vec<String>,
        #[arg(long, help = "Strip text only (no black box overlay)")]
        strip: bool,
    },
    #[command(about = "Render a full SVG document (groups, transforms, shapes, text) to PDF")]
    DrawSvgFile {
        #[arg(help = "Input SVG file")]
        input: String,
        #[arg(short, long, help = "Output PDF file")]
        output: String,
        #[arg(long, help = "Use landscape orientation")]
        landscape: bool,
    },
}

// Use the library instead of declaring modules
use pdfrs::{comprehensive, elements, i18n, image, incremental, linearize, markdown, optimization, parallel, pdf, pdf_generator, pdf_ops, pdf_to_md, plugin, raster, redact, search, security, vector};

fn resolve_locale(cli_lang: &Option<String>) -> i18n::Locale {
    cli_lang
        .as_deref()
        .and_then(i18n::Locale::parse)
        .unwrap_or_else(i18n::Locale::from_env)
}

fn build_plugin_registry(plugins: &str) -> plugin::PluginRegistry {
    let mut registry = plugin::PluginRegistry::new();
    for name in plugins.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match name.to_ascii_lowercase().as_str() {
            "callouts" | "callout" => {
                registry.register_parser(plugin::CalloutPlugin);
                registry.register_generator(plugin::CalloutPlugin);
            }
            other => eprintln!("Warning: unknown plugin '{}'", other),
        }
    }
    registry
}

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
    let locale = resolve_locale(&cli.lang);

    match cli.command {
        Commands::GenerateComprehensive {
            output,
            landscape,
            linearize,
            font_size,
            columns,
        } => {
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
        Commands::PdfToMd { input, output } => match std::fs::read(&input) {
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
                    eprintln!("Structured conversion failed ({}); falling back to plain text", e);
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
        },
        Commands::MdToPdf {
            input,
            output,
            font,
            font_size,
            landscape,
            rtl,
            columns,
            plugins,
            profile,
        } => {
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
                generator.generate(&elements, &output)
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
        Commands::FilterImage {
            input,
            output,
            filters,
            width,
            height,
        } => {
            if filters.is_empty() {
                eprintln!("Error: at least one --filter is required");
                return;
            }
            let parsed: Result<Vec<_>, _> = filters.iter().map(|f| image::ImageFilter::parse(f)).collect();
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
                    println!("Profile: {:?} | compression: {:?} | linearized: {}", profile, settings.compression_level, settings.linearize);
                    if settings.linearize && std::fs::read(&output).ok().is_some_and(|b| linearize::is_linearized(&b)) {
                        println!("Fast Web View: enabled (/Linearized)");
                    }
                }
                Err(e) => eprintln!("Error optimizing PDF: {}", e),
            }
        }
        Commands::LinearizePdf { input, output } => {
            match linearize::linearize_pdf_file(&input, &output) {
                Ok(_) => {
                    println!("Linearized PDF written to {} (Fast Web View)", output);
                }
                Err(e) => eprintln!("Error linearizing PDF: {}", e),
            }
        }
        Commands::IncrementalUpdate {
            input,
            output,
            title,
            author,
            note,
        } => {
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
            certificate,
            cert_id,
            cert_store,
        } => {
            let cert = match (&certificate, &cert_id) {
                (Some(path), _) => Some(security::load_certificate_pem("signing-cert", path)),
                (_, Some(id)) => Some(
                    security::CertificateStore::open(&cert_store).and_then(|store| store.get(id)),
                ),
                _ => None,
            };

            let cert = match cert {
                Some(Ok(c)) => Some(c),
                Some(Err(e)) => {
                    eprintln!("Error loading certificate: {e}");
                    return;
                }
                None => None,
            };

            let signer_name = if signer.is_empty() {
                cert.as_ref()
                    .map(|c| c.subject.clone())
                    .unwrap_or_else(|| "Unknown Signer".to_string())
            } else {
                signer
            };

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let date = format!("{}0000+0000", now);
            let sig = security::DigitalSignature::new(&signer_name).with_date(date);
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
            match pdf_ops::sign_pdf_with_certificate(
                &input,
                &output,
                &sig,
                cert.as_ref(),
            ) {
                Ok(_) => {
                    println!("Successfully signed {} -> {}", input, output);
                    if let Some(c) = &cert {
                        println!("  Certificate: {} ({})", c.id, c.fingerprint_sha256);
                    }
                }
                Err(e) => eprintln!("Error signing PDF: {}", e),
            }
        }
        Commands::ImportCertificate { id, file, subject, store } => {
            match security::CertificateStore::open(&store) {
                Ok(cert_store) => match cert_store.import(&id, &file, subject.as_deref()) {
                    Ok(cert) => {
                        println!("Imported certificate '{}' into {}", id, store);
                        println!("  Subject: {}", cert.subject);
                        println!("  Fingerprint (SHA-256): {}", cert.fingerprint_sha256);
                    }
                    Err(e) => eprintln!("Error importing certificate: {}", e),
                },
                Err(e) => eprintln!("Error opening certificate store: {}", e),
            }
        }
        Commands::ListCertificates { store } => {
            match security::CertificateStore::open(&store) {
                Ok(cert_store) => match cert_store.list() {
                    Ok(certs) => {
                        if certs.is_empty() {
                            println!("No certificates in {}", store);
                        } else {
                            println!("Certificates in {}:", store);
                            for cert in certs {
                                println!(
                                    "  {} — {} [{}]",
                                    cert.id, cert.subject, cert.fingerprint_sha256
                                );
                            }
                        }
                    }
                    Err(e) => eprintln!("Error listing certificates: {}", e),
                },
                Err(e) => eprintln!("Error opening certificate store: {}", e),
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
                            if let Some(ref subject) = sig.certificate_subject {
                                println!("    Certificate subject: {}", subject);
                            }
                            if let Some(ref fp) = sig.certificate_fingerprint {
                                println!("    Certificate fingerprint: {}", fp);
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
                    let result = i18n::localize_validation(locale, &result);
                    println!("{}", i18n::tf(locale, i18n::MsgId::ValidationResultFor, &[&input]));
                    let yes_no = if result.valid {
                        i18n::t(locale, i18n::MsgId::Yes)
                    } else {
                        i18n::t(locale, i18n::MsgId::No)
                    };
                    println!(
                        "  {}: {}",
                        i18n::t(locale, i18n::MsgId::ValidLabel),
                        yes_no
                    );
                    println!(
                        "  {}: {}",
                        i18n::t(locale, i18n::MsgId::PagesLabel),
                        i18n::format_integer(locale, result.page_count as u64)
                    );
                    println!(
                        "  {}: {}",
                        i18n::t(locale, i18n::MsgId::ObjectsLabel),
                        i18n::format_integer(locale, result.object_count as u64)
                    );
                    if !result.errors.is_empty() {
                        println!("  {}:", i18n::t(locale, i18n::MsgId::ErrorsLabel));
                        for e in &result.errors {
                            println!("    - {}", e);
                        }
                    }
                    if !result.warnings.is_empty() {
                        println!("  {}:", i18n::t(locale, i18n::MsgId::WarningsLabel));
                        for w in &result.warnings {
                            println!("    - {}", w);
                        }
                    }
                }
                Err(e) => eprintln!(
                    "{}",
                    i18n::tf(locale, i18n::MsgId::ErrorValidatingPdf, &[&e.to_string()])
                ),
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
        Commands::DrawVector { output, landscape } => {
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
        Commands::DrawSvg {
            output,
            path,
            file,
            landscape,
            line_width,
            fill,
        } => {
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
                (Some(d), _) => match vector::svg_path_to_pdf_bytes(
                    d,
                    layout,
                    stroke,
                    fill_color,
                    line_width,
                ) {
                    Ok(bytes) => std::fs::write(&output, bytes).map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                },
                (None, Some(svg_file)) => vector::svg_file_to_pdf(
                    svg_file,
                    &output,
                    layout,
                    stroke,
                    fill_color,
                    line_width,
                )
                .map_err(|e| e.to_string()),
                (None, None) => Err("Provide --path \"M...\" or --file icon.svg".to_string()),
            };
            match result {
                Ok(_) => println!("Wrote SVG path PDF: {}", output),
                Err(e) => eprintln!("Error rendering SVG path: {}", e),
            }
        }
        Commands::AttachFile { input, output, file, name } => {
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
                        Ok(_) => {
                            match std::fs::write(&output, doc.to_bytes()) {
                                Ok(_) => println!("Attached '{}' to {} as '{}'", file, input, attachment_name),
                                Err(e) => eprintln!("Error writing output PDF: {}", e),
                            }
                        }
                        Err(e) => eprintln!("Error embedding file: {}", e),
                    }
                }
                Err(e) => eprintln!("Error loading PDF: {}", e),
            }
        }
        Commands::Embed3d {
            output,
            model,
            label,
            x,
            y,
            width,
            height,
            activate_on_open,
        } => {
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
        Commands::DiffPdfs { old, new } => {
            match (std::fs::read(&old), std::fs::read(&new)) {
                (Ok(old_bytes), Ok(new_bytes)) => {
                    match pdf::diff_pdf_bytes(&old_bytes, &new_bytes) {
                        Ok(diff) => {
                            println!("PDF diff: {} -> {}", old, new);
                            println!("  Objects: {} -> {}", diff.object_count_old, diff.object_count_new);
                            println!("  Pages: {} -> {}", diff.pages_old, diff.pages_new);
                            println!("  Text similarity: {:.1}%", diff.text_similarity * 100.0);
                            println!("  Added objects: {:?}", diff.added_objects);
                            println!("  Removed objects: {:?}", diff.removed_objects);
                            println!("  Modified objects: {:?}", diff.modified_objects);
                            println!("  Metadata changed: {}", diff.metadata_changed);
                            println!("  Embedded files (old): {}", diff.has_embedded_files_old);
                            println!("  Embedded files (new): {}", diff.has_embedded_files_new);
                        }
                        Err(e) => eprintln!("Error diffing PDFs: {}", e),
                    }
                }
                _ => eprintln!("Error reading one or both PDF files"),
            }
        }
        Commands::SanitizePdf { input, output } => {
            match pdf::PdfDocument::load_from_file(&input) {
                Ok(mut doc) => {
                    doc.sanitize();
                    match std::fs::write(&output, doc.to_bytes()) {
                        Ok(_) => println!("Sanitized PDF written to {}", output),
                        Err(e) => eprintln!("Error writing sanitized PDF: {}", e),
                    }
                }
                Err(e) => eprintln!("Error loading PDF: {}", e),
            }
        }
        Commands::SandboxPdf { input, output } => {
            match std::fs::read(&input) {
                Ok(bytes) => match pdf::sandbox_pdf_bytes(&bytes) {
                    Ok((output_bytes, report)) => match std::fs::write(&output, output_bytes) {
                        Ok(_) => {
                            println!("Sandboxed PDF written to {}", output);
                            println!("  Actions found: {}", report.actions_found.len());
                            println!("  Actions removed: {}", report.actions_removed);
                            println!("  Clean: {}", report.clean);
                            for action in &report.actions_found {
                                let id = action
                                    .object_id
                                    .map(|n| n.to_string())
                                    .unwrap_or_else(|| "?".to_string());
                                println!("    [{}] object {} — {}", action.kind, id, action.description);
                            }
                        }
                        Err(e) => eprintln!("Error writing sandboxed PDF: {}", e),
                    },
                    Err(e) => eprintln!("Error sandboxing PDF: {}", e),
                },
                Err(e) => eprintln!("Error reading PDF: {}", e),
            }
        }
        Commands::ValidatePdfa3 { input } => {
            match pdf::validate_pdf_a3(&input) {
                Ok(result) => {
                    println!("PDF/A-3b validation result for {}:", input);
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
                Err(e) => eprintln!("Error validating PDF/A-3b: {}", e),
            }
        }
        Commands::ValidatePdfua { input } => {
            match pdf::validate_pdf_ua(&input) {
                Ok(result) => {
                    println!("PDF/UA validation result for {}:", input);
                    println!("  Compliant: {}", result.compliant);
                    println!("  MarkInfo: {}", result.has_mark_info);
                    println!("  StructTreeRoot: {}", result.has_struct_tree);
                    println!("  Lang: {}", result.has_lang);
                    println!("  Title: {}", result.has_title);
                    println!("  Fonts embedded: {}", result.fonts_embedded);
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
                Err(e) => eprintln!("Error validating PDF/UA: {}", e),
            }
        }
        Commands::CheckScreenReader { input } => {
            match pdf::check_screen_reader_compliance(&input) {
                Ok(report) => {
                    println!("Screen reader compliance for {}:", input);
                    println!("  Compliant: {}", report.compliant);
                    println!("  Text extractable: {}", report.text_extractable);
                    println!("  Extracted text length: {}", report.extracted_text_length);
                    if !report.structure_element_types.is_empty() {
                        println!(
                            "  Structure types: {}",
                            report.structure_element_types.join(", ")
                        );
                    }
                    if !report.issues.is_empty() {
                        println!("  Issues:");
                        for issue in &report.issues {
                            println!("    - {}", issue);
                        }
                    }
                    if !report.warnings.is_empty() {
                        println!("  Warnings:");
                        for warning in &report.warnings {
                            println!("    - {}", warning);
                        }
                    }
                }
                Err(e) => eprintln!("Error checking screen reader compliance: {}", e),
            }
        }
        Commands::CreatePortfolio { output, files, title } => {
            if files.is_empty() {
                eprintln!("Error: no files provided for portfolio");
                return;
            }
            let file_tuples: Vec<(String, String)> = files.iter()
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
                Ok(_) => println!("Created portfolio PDF with {} file(s): {}", files.len(), output),
                Err(e) => eprintln!("Error creating portfolio: {}", e),
            }
        }
        Commands::WatchMarkdown { input, output, font, font_size, orientation, interval } => {
            let font = font.unwrap_or_else(|| "Helvetica".to_string());
            let font_size = font_size.unwrap_or(12.0);
            let orientation = match orientation.as_deref() {
                Some("landscape") => pdf_generator::PageOrientation::Landscape,
                _ => pdf_generator::PageOrientation::Portrait,
            };
            match markdown::watch_markdown_to_pdf(&input, &output, &font, font_size, orientation, Some(interval)) {
                Ok(_) => {}
                Err(e) => eprintln!("Error watching markdown: {}", e),
            }
        }
        Commands::Repl => {
            cli_repl::run_repl();
        }
        Commands::RasterizePdf { input, output, page, dpi } => {
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
        Commands::SearchPdf { input, query, case_insensitive, json } => {
            let bytes = match std::fs::read(&input) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Error reading PDF: {}", e);
                    return;
                }
            };
            let hits = search::search_text(&bytes, &query, case_insensitive);
            if let Some(path) = json {
                let json_value = serde_json::json!({
                    "query": query,
                    "case_insensitive": case_insensitive,
                    "total": hits.len(),
                    "hits": hits.iter().map(|h| serde_json::json!({
                        "page": h.page,
                        "text": h.text,
                        "snippet": h.snippet,
                        "bbox": {
                            "x": h.bbox.x,
                            "y": h.bbox.y,
                            "width": h.bbox.width,
                            "height": h.bbox.height,
                        },
                    })).collect::<Vec<_>>(),
                });
                let pretty = serde_json::to_string_pretty(&json_value).unwrap_or_default();
                if let Err(e) = std::fs::write(&path, pretty) {
                    eprintln!("Error writing JSON: {}", e);
                }
            }
            if hits.is_empty() {
                println!("No matches for {:?}", query);
            } else {
                println!("Found {} match(es) for {:?}:", hits.len(), query);
                for h in &hits {
                    println!(
                        "  page {} [{:.1},{:.1} {:.1}×{:.1}]: {}",
                        h.page + 1,
                        h.bbox.x,
                        h.bbox.y,
                        h.bbox.width,
                        h.bbox.height,
                        h.snippet
                    );
                }
            }
        }
        Commands::RedactPdf { input, output, region, strip } => {
            if region.is_empty() {
                eprintln!("Error: provide at least one --region page,x,y,w,h");
                return;
            }
            let mut regions = Vec::new();
            for spec in &region {
                let parts: Vec<&str> = spec.split(',').collect();
                if parts.len() != 5 {
                    eprintln!("Error parsing region '{}': expected page,x,y,w,h", spec);
                    return;
                }
                let parsed: Result<Vec<f32>, _> = parts.iter().map(|s| s.trim().parse::<f32>()).collect();
                let nums = match parsed {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("Error parsing region numbers '{}': {}", spec, e);
                        return;
                    }
                };
                regions.push(redact::RedactionRegion {
                    page: nums[0] as usize,
                    x: nums[1],
                    y: nums[2],
                    width: nums[3],
                    height: nums[4],
                });
            }
            let bytes = match std::fs::read(&input) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Error reading PDF: {}", e);
                    return;
                }
            };
            let style = if strip {
                redact::RedactionStyle::Strip
            } else {
                redact::RedactionStyle::BlackBox
            };
            match redact::redact_pdf_bytes_with_style(&bytes, &regions, style) {
                Ok(out) => match std::fs::write(&output, out) {
                    Ok(_) => println!(
                        "Wrote redacted PDF {} ({} region(s), style={:?})",
                        output,
                        regions.len(),
                        style
                    ),
                    Err(e) => eprintln!("Error writing PDF: {}", e),
                },
                Err(e) => eprintln!("Error redacting PDF: {}", e),
            }
        }
        Commands::DrawSvgFile { input, output, landscape } => {
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
}

