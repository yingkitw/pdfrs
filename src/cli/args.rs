//! pdfcli argument definitions (clap derive).

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pdf-cli")]
#[command(about = "A CLI tool to read/write PDFs and convert to/from markdown")]
pub(crate) struct Cli {
    /// UI language for validation/error messages (en, es, de, fr, zh, he, ar).
    /// Falls back to PDFRS_LANG / LANG when omitted.
    #[arg(long, global = true)]
    pub(crate) lang: Option<String>,
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    #[command(about = "Generate the bundled comprehensive capability PDF")]
    GenerateComprehensive {
        #[arg(
            short,
            long,
            help = "Output PDF file",
            default_value = "comprehensive.pdf"
        )]
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
        #[arg(
            long,
            help = "Enable plugins (comma-separated: callouts)",
            default_value = ""
        )]
        plugins: String,
        #[arg(
            long,
            help = "Optimization profile (web, print, archive, ebook)",
            default_value = "archive"
        )]
        profile: String,
    },
    #[command(about = "Convert HTML to PDF")]
    HtmlToPdf {
        #[arg(help = "Input HTML file")]
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
        #[arg(
            long,
            help = "Optimization profile (web, print, archive, ebook)",
            default_value = "archive"
        )]
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
        #[arg(
            long,
            help = "Optimization profile (web, print, archive, ebook)",
            default_value = "archive"
        )]
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
        #[arg(
            long,
            help = "Custom metadata fields (key=value pairs, comma-separated)"
        )]
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
        #[arg(
            short,
            long,
            help = "Optimization profile (web, print, archive, ebook)",
            default_value = "web"
        )]
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
        #[arg(
            long,
            help = "Position (center, topleft, topright, bottomleft, bottomright, diagonal)",
            default_value = "diagonal"
        )]
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
        #[arg(
            short,
            long,
            help = "Output directory for extracted images",
            default_value = "extracted_images"
        )]
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
        #[arg(
            long,
            help = "Encryption algorithm (rc4-40, rc4-128, aes-128, aes-256)",
            default_value = "rc4-128"
        )]
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
        #[arg(
            short,
            long,
            help = "Poll interval in milliseconds",
            default_value = "1000"
        )]
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
        #[arg(
            short,
            long,
            help = "Attachment name in PDF (defaults to file basename)"
        )]
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
        #[arg(
            short,
            long,
            help = "Output PNG file (single page) or directory (all pages)"
        )]
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
