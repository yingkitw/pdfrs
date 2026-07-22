# pdfrs — Pure Rust PDF library & CLI (Markdown ↔ PDF)

**pdfrs** is an open-source **Rust PDF toolkit** and command-line tool (`pdfcli`) for **Markdown to PDF**, **PDF to Markdown**, text extraction, merge/split/rotate, validation, Unicode/CJK documents, charts, and academic thesis layout. It is a **self-contained PDF engine** written in Rust — **no Poppler, no PDFium, no LaTeX** — with an optional **WebAssembly (WASM)** build for the browser.

| | |
|---|---|
| **Crate** | [`pdfrs`](https://crates.io/crates/pdfrs) on crates.io |
| **Docs** | [docs.rs/pdfrs](https://docs.rs/pdfrs) |
| **Binary** | `pdfcli` |
| **License** | [Apache-2.0](LICENSE) |
| **Sample PDF** | [comprehensive.pdf](comprehensive.pdf) — multi-feature demo output |
| **Source Markdown** | [comprehensive_document.md](tests/fixtures/comprehensive_document.md) |

[![crates.io](https://img.shields.io/crates/v/pdfrs.svg)](https://crates.io/crates/pdfrs)
[![docs.rs](https://docs.rs/pdfrs/badge.svg)](https://docs.rs/pdfrs)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-edition%202024-orange.svg)](Cargo.toml)

**See what it can generate:** [**comprehensive.pdf**](comprehensive.pdf)  
Regenerate anytime with `pdfcli generate-comprehensive` or `cargo test --test comprehensive_pdf`.

### At a glance

- **Markdown → PDF** and **PDF → Markdown** in one crate  
- **PDF manipulation**: merge, split, rotate, reorder, watermark, metadata, annotations  
- **Unicode / CJK** with embedded TrueType fonts; optional Base-14 compatibility mode  
- **Charts** (` ```chart` `), **multi-column** layout, **thesis** TOC / Roman folios / citations  
- **Library + CLI + WASM** from the same Rust core  

## Why pdfrs?

Most PDF tools specialize in one lane: a typesetter (Typst/LaTeX), a converter that shells out to a renderer (Pandoc), a low-level object toolkit (lopdf), or a post-processor (qpdf/Ghostscript). **pdfrs** is a single, self-contained Rust stack that covers Markdown → PDF, PDF surgery, validation, and optional WASM — with no system PDF/LaTeX dependency to install.

| Capability | **pdfrs** | Pandoc | Typst | WeasyPrint | lopdf / printpdf | qpdf / Ghostscript |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| Pure Rust PDF engine (no system libs) | ✅ | ❌¹ | ✅ (own format) | ❌ (Python + Pango/Cairo) | ✅ | ❌ (C/C++) |
| Markdown → PDF (built-in) | ✅ | ✅² | Partial (via packages) | ✅ (HTML/CSS path) | ❌ / limited | ❌ |
| Library API *and* CLI in one crate | ✅ | CLI-first | CLI-first | Library-first | Library-first | CLI-first |
| Compile to WASM for the browser | ✅ (`wasm`) | ❌ | Limited | ❌ | Rare | ❌ |
| Merge / split / rotate / watermark | ✅ | ❌ | ❌ | ❌ | DIY | ✅ |
| Linearize (Fast Web View) + incremental update | ✅ | ❌ | ❌ | ❌ | DIY | Partial³ |
| Structural PDF validation API | ✅ | ❌ | ❌ | ❌ | DIY | Partial |
| Unicode / CJK with embedded TTF | ✅ | ✅² | ✅ | ✅ | DIY | N/A |
| Thesis helpers (TOC, Roman folios, citations) | ✅ | Via LaTeX | Via packages | DIY | ❌ | ❌ |
| Markdown charts (` ```chart` `) + multi-column | ✅ | Via filters | Via packages | DIY | ❌ | ❌ |
| Full typographic / TeX-quality math layout | Basic symbols | ✅ (LaTeX) | ✅ | Limited | ❌ | N/A |
| Page rasterization / PDF viewer engine | Preview via pdf.js | N/A | N/A | N/A | ❌ | ✅ (GS) |

¹ Pandoc typically needs a PDF engine (pdflatex, wkhtmltopdf, weasyprint, …).  
² Quality depends on the chosen PDF engine and templates.  
³ qpdf has linearization; Ghostscript is stronger at render/convert than structured incremental edits.

**Choose pdfrs when you want** a dependency-light Rust binary or library that owns the PDF bytes end-to-end — generate from Markdown, tweak existing files, validate, ship the same core to CLI or WASM — without installing TeX or linking PDFium.

**Choose something else when you need** publication-grade typography (Typst/LaTeX), CSS-faithful HTML print (WeasyPrint), or pixel-perfect page rendering (Ghostscript/PDFium).

## Features

### Library API
- **In-memory PDF generation**: `generate_pdf_bytes()` / `generate_pdf_bytes_with_image_base()` — no filesystem needed
- **PDF validation**: `validate_pdf()` / `validate_pdf_bytes()` — structural integrity checks
- **Rich element model**: 27 `Element` variants for document modeling
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
git clone https://github.com/yingkitw/pdfrs.git
cd pdfrs
cargo build --release
```

The binary will be available at `target/release/pdfcli`.

## Usage

### Basic Commands

#### Create a Simple PDF

```bash
pdfcli create output.pdf "Hello, World!"
```

#### Create PDF with Custom Font and Size

```bash
pdfcli create output.pdf "Hello, World!" --font "Times-Roman" --font-size 14
```

#### Convert Markdown to PDF

```bash
pdfcli md-to-pdf input.md output.pdf
```

#### Convert Markdown to PDF with Custom Styling

```bash
pdfcli md-to-pdf input.md output.pdf --font "Helvetica" --font-size 12
```

#### Extract Text from PDF

```bash
pdfcli extract input.pdf
```

#### Convert PDF to Markdown

```bash
pdfcli pdf-to-md input.pdf output.md
```

#### Add Image to PDF

```bash
pdfcli add-image document.pdf image.jpg --x 100 --y 100 --width 200 --height 200
```

#### Filter Image into a PDF

```bash
pdfcli filter-image photo.bmp -o filtered.pdf --filter grayscale --filter brightness:20
```

Supports BMP/PNG with filters: `grayscale`, `invert`, `sepia`, `brightness:N`, `contrast:F`.

#### Draw Vector Graphics Demo

```bash
pdfcli draw-vector diagram.pdf
pdfcli draw-vector diagram.pdf --landscape
```

#### Draw SVG Path

```bash
pdfcli draw-svg out.pdf --path "M72 72 L300 72 L186 220 Z" --fill
pdfcli draw-svg out.pdf --file icon.svg
```

#### Landscape PDF

```bash
pdfcli md-to-pdf input.md output.pdf --landscape
```

#### RTL (Hebrew / Arabic)

```bash
pdfcli md-to-pdf hebrew.md output.pdf --rtl
```

RTL-dominant lines are also auto-detected and right-aligned even without `--rtl`.

#### Embed a U3D 3D model

```bash
pdfcli embed-3d model.pdf scene.u3d --label "Assembly" --activate-on-open
```

#### Localized validation messages

```bash
pdfcli --lang es validate document.pdf
pdfcli --lang de validate document.pdf
```

Supported: `en`, `es`, `de`, `fr`, `zh`, `he`, `ar` (or set `PDFRS_LANG`).

#### Markdown plugins (callouts)

```bash
pdfcli md-to-pdf notes.md out.pdf --plugins callouts
```

Supports fenced callouts: `:::note`, `:::warning`, `:::tip`, `:::danger`, `:::info`.

Headings automatically become PDF bookmarks (`/Outlines`).

#### Generate comprehensive sample PDF

```bash
pdfcli generate-comprehensive
pdfcli generate-comprehensive -o web.pdf --linearize
cargo test --test comprehensive_pdf
```

Produces a multi-page document covering headings/bookmarks, tables, code, math,
callouts, quotes, footnotes, images, charts, multi-column layout, thesis TOC/citations,
page breaks, and light RTL probes. Writes [**comprehensive.pdf**](comprehensive.pdf) by
default (also refreshed when `cargo test --test comprehensive_pdf` runs).

#### Capability showcase (validation fixture)

```bash
# Library + CLI coverage lives in tests/capability_validation.rs
cargo test --test capability_validation

# Generate from the fixture:
pdfcli md-to-pdf tests/fixtures/capability_showcase.md out.pdf --plugins callouts --profile web
pdfcli incremental-update out.pdf -o out2.pdf --title "Updated" --author "Ada"
```

Artifacts are written under `tests/output/capability_*.pdf` when the test runs.

#### Linearize (Fast Web View)

```bash
pdfcli linearize-pdf input.pdf -o web.pdf
pdfcli optimize-pdf input.pdf -o web.pdf --profile web   # also linearizes
```

#### Incremental update (append-only)

```bash
pdfcli incremental-update input.pdf -o updated.pdf --title "New Title" --author "Ada"
pdfcli incremental-update input.pdf -o noted.pdf --note "Please review"
```

#### Merge PDFs

```bash
pdfcli merge file1.pdf file2.pdf file3.pdf -o merged.pdf
```

#### Split PDF (extract pages 2-5)

```bash
pdfcli split input.pdf -o pages2to5.pdf --start 2 --end 5
```

#### Rotate PDF

```bash
pdfcli rotate input.pdf -o rotated.pdf --angle 90
```

#### Create PDF with Metadata

```bash
pdfcli md-to-pdf-meta input.md output.pdf --title "My Document" --author "Author Name" --subject "Topic"
```

### Supported Fonts

- Helvetica
- Times-Roman
- Courier
- And other standard PDF Type 1 fonts

## Examples

### Creating a Multi-page Document

```bash
pdfcli create long-document.pdf "$(cat document.txt)" --font-size 10
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

pdfcli md-to-pdf sample.md sample.pdf --font "Times-Roman" --font-size 12

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
- **PDF Generator** (`src/pdf_generator.rs`): Creates PDFs with layout, color, alignment, accessibility, syntect highlighting
- **Elements** (`src/elements.rs`): 27 structured element types and markdown parser
- **Markdown** (`src/markdown.rs`): Markdown-to-PDF pipeline with rich formatting
- **Charts / thesis** (`src/chart.rs`, `src/thesis.rs`): Vector charts; TOC, folios, citations
- **PDF Operations** (`src/pdf_ops.rs`): Merge, split, rotate, reorder, watermark, metadata, annotations
- **Image Handler** (`src/image.rs`): JPEG/PNG/BMP embedding with dimension parsing
- **Linearize / incremental** (`src/linearize.rs`, `src/incremental.rs`): Fast Web View; append-only updates
- **Plugins** (`src/plugin.rs`): Parser/generator hooks (e.g. callouts)
- **Compression** (`src/compression.rs`): PDF stream compression (deflate)
- **Security** (`src/security.rs`): Password protection, permissions (stub crypto gated)

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed module documentation. Spec and backlog: [SPEC.md](SPEC.md), [TODO.md](TODO.md).

## Testing

~336 tests (`cargo test`), including:
- **~240 lib tests**: Unit tests across modules
- **Integration crates**: `tests/integration.rs`, `comprehensive_pdf`, `roundtrip_test`, `capability_validation`, `unicode_integration_test`
- **~33 doctests**: Public API examples

Round-trip validation tests verify that content survives: generate → validate → parse → verify.

```bash
cargo test
```

## Documentation

Project docs live at the repository root:

| File | Purpose |
|------|---------|
| [README.md](README.md) | Quick start, CLI usage, features |
| [SPEC.md](SPEC.md) | Functional / non-functional requirements |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Modules, data flow, design decisions |
| [TODO.md](TODO.md) | Backlog, audit follow-ups, brainstorming |

Extra material (contributing, validation notes) is under [`docs/`](docs/).

## Limitations

- Text extraction works best with PDFs generated by this tool or simple Type 1 font PDFs
- Font support is limited to standard Type 1 fonts (Helvetica, Times-Roman, Courier) unless a Unicode TTF is embedded
- Chart fences cover bar / line / pie only (no stacked or multi-series charts yet)
- Full tagged PDF output not yet implemented (structure types defined)

## FAQ

### What is pdfrs?
**pdfrs** is a pure Rust PDF library and CLI (`pdfcli`) that converts Markdown to PDF, extracts or converts PDF to Markdown, and performs PDF operations (merge, split, rotate, validate, linearize) without linking Poppler, PDFium, or requiring LaTeX.

### How is pdfrs different from Pandoc or Typst?
Pandoc usually needs an external PDF engine; Typst is a full typesetting language. pdfrs is a **self-contained PDF toolkit** focused on Markdown ↔ PDF plus PDF surgery, validation, and WASM — one Rust crate for library and CLI use.

### Does pdfrs support Chinese, Japanese, and Korean (CJK)?
Yes. Non-ASCII documents embed a Unicode TrueType font as Type0/CIDFont by default. Override the font with `PDFRS_UNICODE_FONT_PATH`.

### Can I generate PDFs in the browser?
Yes. Build with the `wasm` feature (`./scripts/build-wasm.sh`) and use the JavaScript API; see [wasm/README.md](wasm/README.md).

### Where can I see sample output?
Download or open [comprehensive.pdf](comprehensive.pdf) in this repository.

## Contributing

Contributions are welcome! Please read our [Contributing Guidelines](docs/CONTRIBUTING.md) for details.

## License

This project is licensed under the Apache License 2.0 — see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Built entirely in Rust without external PDF dependencies
- Implements core PDF specifications from scratch
- Inspired by the need for a lightweight PDF toolchain
