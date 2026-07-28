# pdfrs User Guide

A practical guide to using **pdfrs** (`pdfcli`) for PDF generation, manipulation, and analysis.

---

## Installation

### From source

```bash
git clone https://github.com/yingkitw/pdfrs.git
cd pdfrs
cargo install --path .
```

### From crates.io

```bash
cargo install pdfrs
```

### As a library dependency

Add to your `Cargo.toml`:

```toml
[dependencies]
pdfrs = { version = "0.1", default-features = false }
```

#### Feature flags

| Feature | Default | Description |
|---|---|---|
| `parallel` | yes | Rayon-based parallel processing (merge, batch generate) |
| `wasm` | no | WebAssembly build (`wasm-bindgen`, no filesystem) |
| `async` | no | Async API via Tokio for web server integration |

---

## CLI Quick Reference

All commands are invoked via the `pdfcli` binary. Run `pdfcli --help` for the full list.

### Global options

- `--lang <CODE>` — UI language for messages (`en`, `es`, `de`, `fr`, `zh`, `he`, `ar`). Falls back to `PDFRS_LANG` / `LANG` env vars.

### Conversion

| Command | Description |
|---|---|
| `md-to-pdf <input.md> <output.pdf>` | Convert Markdown to PDF |
| `pdf-to-md <input.pdf> <output.md>` | Convert PDF to structured Markdown |
| `html-to-pdf <input.html> <output.pdf>` | Convert HTML to PDF |
| `md-to-pdf-meta <input.md> <output.pdf>` | Markdown to PDF with metadata (title, author, etc.) |

**Common flags:** `--font`, `--font-size`, `--landscape`, `--rtl`, `--columns N`, `--profile web|print|archive|ebook`, `--plugins callouts`

### Creation

| Command | Description |
|---|---|
| `create <output.pdf> "text"` | Create a simple PDF from text |
| `create-streaming <output.pdf> "text"` | Memory-efficient streaming creation |
| `generate-comprehensive` | Generate the bundled multi-feature demo PDF |

### Text & Structure

| Command | Description |
|---|---|
| `extract <input.pdf>` | Extract plain text from a PDF |
| `detect-structure <input.pdf>` | Detect headings and sections |
| `extract-tables <input.pdf> -o <output.csv>` | Extract tables to CSV |
| `search-pdf <input.pdf> "query"` | Full-text search with bounding boxes |
| `diff-pdfs <old.pdf> <new.pdf>` | Compare two PDFs structurally |

### Manipulation

| Command | Description |
|---|---|
| `merge <a.pdf> <b.pdf> -o <merged.pdf>` | Merge multiple PDFs |
| `split <input.pdf> -o <output.pdf> --start 1 --end 5` | Extract page range |
| `reorder <input.pdf> -o <output.pdf> --pages 3,1,2` | Reorder pages |
| `rotate <input.pdf> -o <output.pdf> --angle 90` | Rotate all pages |
| `watermark <input.pdf> -o <output.pdf> --text "DRAFT"` | Add text watermark |
| `watermark-advanced <input.pdf> -o <output.pdf> --text "CONFIDENTIAL" --position diagonal --opacity 0.3` | Advanced watermarking |
| `overlay-image <input.pdf> -o <output.pdf> --image logo.png --x 100 --y 100` | Overlay image on all pages |
| `add-image <input.pdf> <image.png>` | Add image to PDF |
| `filter-image <input.png> -o <output.pdf> --filter grayscale --filter brightness:50` | Image filters to PDF |
| `attach-file <input.pdf> -o <output.pdf> <file.zip>` | Attach external file |
| `create-portfolio -o <portfolio.pdf> file1.pdf file2.pdf` | Create PDF portfolio |
| `incremental-update <input.pdf> -o <output.pdf> --title "New Title"` | Append incremental update |

### Optimization

| Command | Description |
|---|---|
| `optimize-pdf <input.pdf> -o <output.pdf> --profile web` | Recompress and reduce size |
| `linearize-pdf <input.pdf> -o <output.pdf>` | Fast Web View (progressive loading) |

### Security

| Command | Description |
|---|---|
| `sanitize-pdf <input.pdf> -o <output.pdf>` | Remove JavaScript, launch actions, external refs |
| `sandbox-pdf <input.pdf> -o <output.pdf>` | Strip JavaScript actions with report |
| `protect <input.pdf> -o <output.pdf> --user-password secret` | Add password protection |
| `sign <input.pdf> <output.pdf> --signer "Alice" --reason "Approved"` | Add digital signature |
| `verify-signature <input.pdf>` | Verify digital signatures |
| `import-certificate <id> <cert.pem> --store certs/` | Import X.509 certificate |
| `list-certificates --store certs/` | List stored certificates |

### Validation

| Command | Description |
|---|---|
| `validate <input.pdf>` | Structural integrity check |
| `validate-pdfa <input.pdf>` | PDF/A-1b compliance |
| `validate-pdfa3 <input.pdf>` | PDF/A-3b compliance |
| `validate-pdfua <input.pdf>` | PDF/UA accessibility compliance |
| `check-screen-reader <input.pdf>` | Screen reader compliance |

### Rendering & Graphics

| Command | Description |
|---|---|
| `rasterize-pdf <input.pdf> -o <output_dir> --dpi 150` | Rasterize pages to PNG |
| `draw-vector <output.pdf>` | Generate vector graphics demo PDF |
| `draw-svg <output.pdf> --path "M 10 10 L 100 100"` | Render SVG path to PDF |
| `draw-svg-file <input.svg> -o <output.pdf>` | Render full SVG document to PDF |
| `embed-3d <output.pdf> <model.u3d>` | Embed U3D 3D annotation |

### Forms

| Command | Description |
|---|---|
| `create-form <output.pdf> "text" --fields fields.json` | Create PDF with form fields |
| `detect-form-fields <input.pdf>` | Detect existing form fields |
| `fill-form-fields <input.pdf> -o <output.pdf> --values '{"name":"Alice"}'` | Fill form fields |

### Images

| Command | Description |
|---|---|
| `extract-images <input.pdf> -o <output_dir>` | Extract embedded images |

### Developer Tools

| Command | Description |
|---|---|
| `watch-markdown <input.md> -o <output.pdf> --interval 500` | Hot-reload on file changes |
| `repl` | Interactive REPL for PDF manipulation |
| `redact-pdf <input.pdf> -o <output.pdf> --region 1,100,100,200,50` | Redact regions (content-stream rewrite) |

---

## Library API

### Markdown to PDF

```rust
use pdfrs::{elements, pdf_generator};

let md = "# Hello\n\nThis is **bold** text.";
let elements = elements::parse_markdown(md);
let layout = pdf_generator::PageLayout::portrait();

pdf_generator::create_pdf_from_elements_with_layout(
    "output.pdf",
    &elements,
    "Helvetica",
    12.0,
    layout,
).expect("Failed to create PDF");
```

### In-memory generation (no filesystem)

```rust
use pdfrs::{elements, pdf_generator};

let elements = elements::parse_markdown("# Test\n\nContent here.");
let layout = pdf_generator::PageLayout::portrait();

let bytes: Vec<u8> = pdf_generator::generate_pdf_bytes(
    &elements,
    "Helvetica",
    12.0,
    layout,
).expect("Failed");

// bytes is a complete PDF file — save, serve, or send it
```

### Builder API

```rust
use pdfrs::{builder::PdfBuilder, pdf_generator::PageLayout};

let pdf_bytes = PdfBuilder::new()
    .with_layout(PageLayout::portrait())
    .with_font("Helvetica")
    .with_font_size(12.0)
    .add_heading("Document Title", 1)
    .add_paragraph("This is the first paragraph.")
    .add_code_block("println!(\"Hello\");", "rust")
    .add_list_item("First item", 0)
    .add_list_item("Second item", 0)
    .add_horizontal_rule()
    .add_page_break()
    .add_heading("Page Two", 2)
    .build_bytes()
    .expect("Failed to build PDF");
```

### PDF parsing and text extraction

```rust
use pdfrs::pdf::PdfDocument;

let doc = PdfDocument::load_from_file("input.pdf").expect("Failed to load");
let text = doc.get_text().expect("Failed to extract");
println!("{}", text);
```

### PDF manipulation

```rust
use pdfrs::pdf_ops;

// Merge
pdf_ops::merge_pdfs(&["a.pdf", "b.pdf"], "merged.pdf").expect("Failed to merge");

// Split (pages 1-3)
pdf_ops::split_pdf("input.pdf", "pages_1_3.pdf", 1, 3).expect("Failed to split");

// Rotate 90 degrees
pdf_ops::rotate_pdf("input.pdf", "rotated.pdf", 90).expect("Failed to rotate");
```

### Validation

```rust
use pdfrs::pdf;

let result = pdf::validate_pdf("input.pdf").expect("Failed to validate");
if result.is_valid {
    println!("PDF is valid ({} pages, {} objects)", result.page_count, result.object_count);
} else {
    for err in &result.errors {
        eprintln!("Error: {}", err);
    }
}
```

### HTML to PDF

```rust
use pdfrs::html;

let html = "<h1>Title</h1><p>Paragraph text</p>";
let bytes = html::html_to_pdf_bytes(html, "Helvetica", 12.0, Default::default())
    .expect("Failed to convert");
```

### PDF to Markdown

```rust
use pdfrs::pdf_to_md;

let pdf_bytes = std::fs::read("input.pdf").unwrap();
let markdown = pdf_to_md::pdf_bytes_to_markdown(&pdf_bytes).expect("Failed to convert");
println!("{}", markdown);
```

### Search with bounding boxes

```rust
use pdfrs::search;

let pdf_bytes = std::fs::read("input.pdf").unwrap();
let hits = search::search_pdf_bytes(&pdf_bytes, "important", true).expect("Failed to search");
for hit in hits {
    println!("Page {}: '{}' at ({}, {})", hit.page, hit.text, hit.bbox.x, hit.bbox.y);
}
```

### Redaction

```rust
use pdfrs::redact;

let pdf_bytes = std::fs::read("input.pdf").unwrap();
let regions = vec![redact::RedactionRegion {
    page: 0,
    x: 100.0, y: 100.0, width: 200.0, height: 50.0,
}];
let redacted = redact::redact_pdf_bytes(&pdf_bytes, &regions, redact::RedactionStyle::BlackBox)
    .expect("Failed to redact");
```

### Rasterization

```rust
use pdfrs::raster;

let pdf_bytes = std::fs::read("input.pdf").unwrap();
let images = raster::rasterize_all(&pdf_bytes, 150).expect("Failed to rasterize");
for (i, png) in images.iter().enumerate() {
    std::fs::write(format!("page_{}.png", i + 1), png).unwrap();
}
```

### Plugins

```rust
use pdfrs::{elements, plugin};

let md = ":::note\nThis is a callout.\n:::\n\nRegular text.";
let registry = plugin::PluginRegistry::new()
    .with_parser(plugin::CalloutPlugin)
    .with_generator(plugin::CalloutPlugin);
let elements = plugin::parse_markdown_with_plugins(md, &registry);
```

---

## Common Workflows

### 1. Markdown report to optimized PDF

```bash
pdfcli md-to-pdf report.md report.pdf --profile web --columns 2
pdfcli linearize-pdf report.pdf report_fast.pdf
```

### 2. Merge and watermark

```bash
pdfcli merge chapter1.pdf chapter2.pdf chapter3.pdf -o full.pdf
pdfcli watermark full.pdf -o final.pdf --text "DRAFT" --opacity 0.2
```

### 3. Extract and search

```bash
pdfcli extract input.pdf > text.txt
pdfcli search-pdf input.pdf "invoice" --json results.json
pdfcli extract-tables input.pdf -o tables.csv
```

### 4. Sanitize untrusted PDF

```bash
pdfcli sanitize-pdf untrusted.pdf -o clean.pdf
pdfcli validate clean.pdf
```

### 5. Redact sensitive content

```bash
pdfcli redact-pdf input.pdf -o redacted.pdf --region 1,100,200,300,50 --region 1,100,300,300,50
```

### 6. Batch rasterize

```bash
pdfcli rasterize-pdf input.pdf -o pages/ --dpi 300
```

### 7. Interactive REPL

```bash
pdfcli repl
> load document.pdf
> text
> pages
> validate
> optimize optimized.pdf
> quit
```

---

## Unicode & CJK Support

pdfrs supports Unicode text (including CJK: Chinese, Japanese, Korean) via an embedded TrueType font. Set the font path with the `PDFRS_UNICODE_FONT_PATH` environment variable if you need a specific font:

```bash
export PDFRS_UNICODE_FONT_PATH=/path/to/font.ttf
pdfcli md-to-pdf chinese_doc.md output.pdf
```

Without a custom font, a bundled font is used automatically.

---

## Environment Variables

| Variable | Description |
|---|---|
| `PDFRS_UNICODE_FONT_PATH` | Path to TrueType font for Unicode/CJK rendering |
| `PDFRS_LANG` | UI language (same as `--lang`) |
| `LANG` | Fallback locale if `PDFRS_LANG` not set |

---

## See Also

- [README.md](../README.md) — Project overview and feature list
- [ARCHITECTURE.md](../ARCHITECTURE.md) — Module structure and data flow
- [SPEC.md](../SPEC.md) — Functional requirements and protocol semantics
- [CONTRIBUTING.md](./CONTRIBUTING.md) — How to contribute
- [API docs](https://docs.rs/pdfrs) — Full rustdoc API reference
