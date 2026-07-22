# PDF-CLI

A Rust library and CLI tool for reading, writing, and manipulating PDF files. Converts to/from Markdown. Implemented entirely in Rust without external PDF libraries.

## Features

### Library API
- **In-memory PDF generation**: `generate_pdf_bytes()` — no filesystem needed
- **PDF validation**: `validate_pdf()` / `validate_pdf_bytes()` — structural integrity checks
- **Rich element model**: 17 `Element` variants for document modeling
- **Accessibility**: `StructureType` enum (35 types), `StructureElement` tree, `AccessibilityOptions`

### PDF Generation
- **From scratch**: Create PDFs with custom fonts and text content
- **From Markdown**: Rich formatting (headers, lists, task lists, blockquotes, tables, code blocks, definition lists, footnotes, images, links, page breaks)
- **Math rendering**: Supports inline `$...$` and `$$...$$` blocks with LaTeX-like symbol conversion and glyph-safe Unicode symbol output (including italic math text)
- **Unicode-aware wrapping**: Better line wrapping/width estimation for CJK and emoji-heavy content
- **Embedded Unicode font (default)**: Uses a Type0/CIDFont with embedded TrueType (`FontFile2`) and glyph-ID text encoding for correct cross-script rendering
- **Unicode font path override**: Set `PDFRS_UNICODE_FONT_PATH=/path/to/font.ttf` to control which Unicode TTF is embedded
- **Base-14 font compatibility mode (opt-in)**: Set `PDFRS_BASE14_NORMALIZE=1` to normalize non-ASCII glyphs for Helvetica/Courier-only PDFs (math/currency transliteration + `[U+XXXX]` fallback)
- **Text color**: `Color` struct (RGB), code blocks in gray, links in blue
- **Text alignment**: H1 centered, configurable `TextAlign` enum
- **Page orientation**: Landscape/portrait with `--landscape` CLI flag
- **Page numbering**: Automatic footer page numbers
- **Watermarks**: Diagonal text with configurable opacity/size

### PDF Parsing
- **Text extraction**: Tj, TJ operators, font encodings (WinAnsi, MacRoman)
- **Cross-reference streams**: PDF 1.5+ xref stream parsing
- **Object streams**: Compressed object stream handling
- **Validation**: Header, xref, trailer, catalog, pages, object pairing checks

### PDF Manipulation
- **Merge**: Combine multiple PDFs
- **Split**: Extract page ranges
- **Rotate**: 0/90/180/270°
- **Reorder**: Arbitrary page ordering
- **Watermark**: Diagonal text overlay
- **Metadata**: Title, author, subject, keywords
- **Annotations**: Text, link, and highlight annotations
- **Images**: JPEG/PNG/BMP embedding (CLI + Markdown `![alt](path)`)
- **Charts**: Markdown fenced `chart` blocks (bar / line / pie)
- **Multi-column**: `<!-- columns:N -->` and `--columns`
- **Thesis**: TOC, Roman/Arabic folios, running headers, citations/bibliography

## Installation

### From Source

```bash
git clone https://github.com/yourusername/pdf-cli.git
cd pdf-cli
cargo build --release
```

The binary will be available at `target/release/pdf-cli`.

## Usage

### Basic Commands

#### Create a Simple PDF

```bash
pdf-cli create output.pdf "Hello, World!"
```

#### Create PDF with Custom Font and Size

```bash
pdf-cli create output.pdf "Hello, World!" --font "Times-Roman" --font-size 14
```

#### Convert Markdown to PDF

```bash
pdf-cli md-to-pdf input.md output.pdf
```

#### Convert Markdown to PDF with Custom Styling

```bash
pdf-cli md-to-pdf input.md output.pdf --font "Helvetica" --font-size 12
```

#### Extract Text from PDF

```bash
pdf-cli extract input.pdf
```

#### Convert PDF to Markdown

```bash
pdf-cli pdf-to-md input.pdf output.md
```

#### Add Image to PDF

```bash
pdf-cli add-image document.pdf image.jpg --x 100 --y 100 --width 200 --height 200
```

#### Filter Image into a PDF

```bash
pdf-cli filter-image photo.bmp -o filtered.pdf --filter grayscale --filter brightness:20
```

Supports BMP/PNG with filters: `grayscale`, `invert`, `sepia`, `brightness:N`, `contrast:F`.

#### Draw Vector Graphics Demo

```bash
pdf-cli draw-vector diagram.pdf
pdf-cli draw-vector diagram.pdf --landscape
```

#### Draw SVG Path

```bash
pdf-cli draw-svg out.pdf --path "M72 72 L300 72 L186 220 Z" --fill
pdf-cli draw-svg out.pdf --file icon.svg
```

#### Landscape PDF

```bash
pdf-cli md-to-pdf input.md output.pdf --landscape
```

#### RTL (Hebrew / Arabic)

```bash
pdf-cli md-to-pdf hebrew.md output.pdf --rtl
```

RTL-dominant lines are also auto-detected and right-aligned even without `--rtl`.

#### Embed a U3D 3D model

```bash
pdf-cli embed-3d model.pdf scene.u3d --label "Assembly" --activate-on-open
```

#### Localized validation messages

```bash
pdf-cli --lang es validate document.pdf
pdf-cli --lang de validate document.pdf
```

Supported: `en`, `es`, `de`, `fr`, `zh`, `he`, `ar` (or set `PDFRS_LANG`).

#### Markdown plugins (callouts)

```bash
pdf-cli md-to-pdf notes.md out.pdf --plugins callouts
```

Supports fenced callouts: `:::note`, `:::warning`, `:::tip`, `:::danger`, `:::info`.

Headings automatically become PDF bookmarks (`/Outlines`).

#### Generate comprehensive sample PDF

```bash
pdf-cli generate-comprehensive
pdf-cli generate-comprehensive -o web.pdf --linearize
cargo test --test comprehensive_pdf
```

Produces a multi-page document covering headings/bookmarks, tables, code, math,
callouts, quotes, footnotes, page breaks, and light RTL probes. Writes
`comprehensive.pdf` by default (also refreshed when the test runs).

#### Capability showcase (validation fixture)

```bash
# Library + CLI coverage lives in tests/capability_validation.rs
cargo test --test capability_validation

# Generate from the fixture:
pdf-cli md-to-pdf tests/fixtures/capability_showcase.md out.pdf --plugins callouts --profile web
pdf-cli incremental-update out.pdf -o out2.pdf --title "Updated" --author "Ada"
```

Artifacts are written under `tests/output/capability_*.pdf` when the test runs.

#### Linearize (Fast Web View)

```bash
pdf-cli linearize-pdf input.pdf -o web.pdf
pdf-cli optimize-pdf input.pdf -o web.pdf --profile web   # also linearizes
```

#### Incremental update (append-only)

```bash
pdf-cli incremental-update input.pdf -o updated.pdf --title "New Title" --author "Ada"
pdf-cli incremental-update input.pdf -o noted.pdf --note "Please review"
```

#### Merge PDFs

```bash
pdf-cli merge file1.pdf file2.pdf file3.pdf -o merged.pdf
```

#### Split PDF (extract pages 2-5)

```bash
pdf-cli split input.pdf -o pages2to5.pdf --start 2 --end 5
```

#### Rotate PDF

```bash
pdf-cli rotate input.pdf -o rotated.pdf --angle 90
```

#### Create PDF with Metadata

```bash
pdf-cli md-to-pdf-meta input.md output.pdf --title "My Document" --author "Author Name" --subject "Topic"
```

### Supported Fonts

- Helvetica
- Times-Roman
- Courier
- And other standard PDF Type 1 fonts

## Examples

### Creating a Multi-page Document

```bash
pdf-cli create long-document.pdf "$(cat document.txt)" --font-size 10
```

### Converting Complex Markdown

````bash
# Create a sample markdown file
cat > sample.md << EOF
# Sample Document

This is a **bold** text with *italic* formatting.

## Tables

| Name | Age | Country |
|------|-----|---------|
| John | 25  | USA     |
| Jane | 30  | UK      |

### Lists

1. First item
2. Second item
   - Nested item
   - Another nested item

### Code Examples

```rust
fn main() {
    println!("Hello, PDF!");
}
````

EOF

# Convert to PDF

pdf-cli md-to-pdf sample.md sample.pdf --font "Times-Roman" --font-size 12

```

## Library Usage

```rust
use pdfrs::{elements, pdf_generator, pdf};

// Parse markdown into elements
let elements = elements::parse_markdown("# Hello\n\nWorld");

// Generate PDF bytes in memory
let layout = pdf_generator::PageLayout::portrait();
let pdf_bytes = pdf_generator::generate_pdf_bytes(
    &elements, "Helvetica", 12.0, layout
).unwrap();

// Validate the generated PDF
let validation = pdf::validate_pdf_bytes(&pdf_bytes);
assert!(validation.valid);
assert!(validation.page_count >= 1);
```

## WebAssembly

Generate PDFs in the browser with the optional `wasm` feature:

```bash
./scripts/build-wasm.sh
python3 -m http.server 8080
# open http://localhost:8080/wasm/example.html
```

The demo generates PDFs from Markdown via WASM and previews them on canvas. See [wasm/README.md](wasm/README.md) for the JavaScript API and viewer utilities.

## Architecture

This tool is built with a modular architecture:

- **PDF Parser** (`src/pdf.rs`): PDF parsing, text extraction, validation, xref/object stream parsing
- **PDF Generator** (`src/pdf_generator.rs`): Creates PDFs with layout, color, alignment, accessibility
- **Elements** (`src/elements.rs`): 17 structured element types and markdown parser
- **Markdown** (`src/markdown.rs`): Markdown-to-PDF pipeline with rich formatting
- **PDF Operations** (`src/pdf_ops.rs`): Merge, split, rotate, reorder, watermark, metadata, annotations
- **Image Handler** (`src/image.rs`): JPEG/PNG/BMP embedding with dimension parsing
- **Compression** (`src/compression.rs`): PDF stream compression (deflate)
- **Security** (`src/security.rs`): Password protection, permissions

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for detailed module documentation.

## Testing

251 tests across 4 test suites:
- **115 lib tests**: Unit tests for all modules
- **112 bin tests**: CLI command tests
- **13 integration tests**: End-to-end roundtrip, merge, split, rotate, watermark, reorder
- **11 bench tests**: Property-based and benchmark tests

Round-trip validation tests verify that every element type survives: generate → validate → parse → verify.

```bash
cargo test
```

## Limitations

- Text extraction works best with PDFs generated by this tool or simple Type 1 font PDFs
- Font support is limited to standard Type 1 fonts (Helvetica, Times-Roman, Courier) unless a Unicode TTF is embedded
- Chart fences cover bar / line / pie only (no stacked or multi-series charts yet)
- Full tagged PDF output not yet implemented (structure types defined)

## Contributing

Contributions are welcome! Please read our [Contributing Guidelines](docs/CONTRIBUTING.md) for details.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Built entirely in Rust without external PDF dependencies
- Implements core PDF specifications from scratch
- Inspired by the need for a lightweight PDF toolchain
