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
[![CI](https://github.com/yingkitw/pdfrs/actions/workflows/ci.yml/badge.svg)](https://github.com/yingkitw/pdfrs/actions/workflows/ci.yml)

**See what it can generate:** [**comprehensive.pdf**](comprehensive.pdf)  
Regenerate anytime with `pdfcli generate-comprehensive` or `cargo test --test comprehensive_pdf`.

### At a glance

- **Markdown → PDF** and **PDF → Markdown** (structured: headings, lists, code blocks)
- **PDF manipulation**: merge, split, rotate, reorder, watermark, metadata, annotations
- **Search** with per-hit bounding boxes; **true redaction** that rewrites content streams
- **Rasterize** PDF → PNG (pure Rust, no external renderer)
- **Full SVG documents** (`<g transform>`, shapes, `<text>`) → PDF
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
| Page rasterization / PDF viewer engine | ✅ Schematic PNG (`rasterize-pdf`) | N/A | N/A | N/A | ❌ | ✅ (GS) |

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
- **Full SVG documents**: `<g transform>`, `<rect>`, `<circle>`, `<ellipse>`, `<line>`, `<polygon>`, `<path>`, `<text>` with `fill`/`stroke`/`stroke-width` — `draw-svg-file` CLI
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
- **Search**: Full-text search with per-hit bounding boxes (`search-pdf`)
- **Redact**: True content-stream redaction — rewrites streams to remove text (`redact-pdf`)
- **Rasterize**: PDF → PNG without external renderers (`rasterize-pdf`)

## Install

```bash
git clone https://github.com/yingkitw/pdfrs.git
cd pdfrs
cargo build --release
# binary: ./target/release/pdfcli
# optional: cargo install --path .
```

## Quick start

Three commands cover most work:

```bash
# 1) Markdown → PDF
pdfcli md-to-pdf notes.md notes.pdf

# 2) Read a PDF back as text / Markdown
pdfcli extract notes.pdf
pdfcli pdf-to-md notes.pdf notes.md

# 3) Try the full demo document
pdfcli generate-comprehensive          # writes comprehensive.pdf
```

Add the release binary to your `PATH`, or run it as `./target/release/pdfcli …`.

## Everyday CLI

| Do this | Command |
|---|---|
| Markdown → PDF | `pdfcli md-to-pdf in.md out.pdf` |
| PDF → Markdown (structured) | `pdfcli pdf-to-md in.pdf out.md` |
| Extract text | `pdfcli extract in.pdf` |
| Create from a string | `pdfcli create out.pdf "Hello"` |
| Merge | `pdfcli merge a.pdf b.pdf -o out.pdf` |
| Split pages 2–5 | `pdfcli split in.pdf -o out.pdf --start 2 --end 5` |
| Rotate 90° | `pdfcli rotate in.pdf -o out.pdf --angle 90` |
| Validate | `pdfcli validate in.pdf` |
| Search with bbox | `pdfcli search-pdf in.pdf "needle" --json hits.json` |
| Redact regions | `pdfcli redact-pdf in.pdf -o out.pdf --region 0,100,700,200,20` |
| Rasterize to PNG | `pdfcli rasterize-pdf in.pdf -o page.png --dpi 96` |
| Render SVG file | `pdfcli draw-svg-file drawing.svg -o drawing.pdf` |
| Sample showcase | `pdfcli generate-comprehensive` |

### Useful flags (md-to-pdf)

```bash
pdfcli md-to-pdf in.md out.pdf --font Helvetica --font-size 12
pdfcli md-to-pdf in.md out.pdf --landscape
pdfcli md-to-pdf in.md out.pdf --columns 2
pdfcli md-to-pdf in.md out.pdf --rtl                 # Hebrew / Arabic
pdfcli md-to-pdf in.md out.pdf --plugins callouts    # :::note / :::warning / …
pdfcli md-to-pdf-meta in.md out.pdf --title "Doc" --author "Ada"
```

Fonts: `Helvetica`, `Times-Roman`, `Courier` (plus other Base-14 names). Non-ASCII/CJK embeds a Unicode TTF automatically.

### PDF → Markdown
- **Structured conversion**: reconstructs headings (font-size ratios), bullets,
  numbered lists, fenced code blocks (Courier detection), and horizontal rules
- **ToUnicode-aware**: decodes embedded CID fonts via the document's
  ToUnicode CMap so glyph-ID hex strings come back as readable text
- **CLI**: `pdfcli pdf-to-md input.pdf output.md`

### More commands

```bash
pdfcli merge a.pdf b.pdf -o out.pdf
pdfcli split in.pdf -o out.pdf --start 2 --end 5
pdfcli rotate in.pdf -o out.pdf --angle 90
pdfcli linearize-pdf in.pdf -o web.pdf
pdfcli incremental-update in.pdf -o out.pdf --title "Updated" --author "Ada"
pdfcli add-image doc.pdf photo.jpg --x 100 --y 100 --width 200 --height 200
pdfcli --lang es validate doc.pdf          # en es de fr zh he ar
pdfcli rasterize-pdf doc.pdf -o page.png            # PDF → PNG (no pdf.js)
pdfcli rasterize-pdf doc.pdf -o pages/              # all pages → pages/page-NNNN.png
pdfcli search-pdf doc.pdf "needle" --case-insensitive --json hits.json
pdfcli redact-pdf doc.pdf -o out.pdf --region 0,100,700,200,20
pdfcli draw-svg-file drawing.svg -o drawing.pdf     # full SVG document
pdfcli --help                              # full command list
```

## Library (Rust)

Add to `Cargo.toml`: `pdfrs = "0.2"`

```rust
use pdfrs::{elements, pdf, pdf_generator::{generate_pdf_bytes, PageLayout}};

fn main() -> anyhow::Result<()> {
    let elements = elements::parse_markdown("# Hello\n\nFrom **pdfrs**.");
    let pdf_bytes = generate_pdf_bytes(&elements, "Helvetica", 12.0, PageLayout::portrait())?;
    assert!(pdf::validate_pdf_bytes(&pdf_bytes).valid);
    std::fs::write("hello.pdf", pdf_bytes)?;
    Ok(())
}
```

Relative images in Markdown: use `generate_pdf_bytes_with_image_base(...)` with the markdown file’s directory.

### Search, redact, rasterize, SVG

```rust
use pdfrs::{raster, redact, search};

// Search a PDF for every occurrence of `needle` with bbox per hit.
let pdf = std::fs::read("doc.pdf")?;
for hit in search::search_text(&pdf, "needle", true) {
    println!("page {} {}: {}", hit.page, hit.bbox.width, hit.snippet);
}

// Redact a region (rewrites content streams; default is black-box overlay).
let redacted = redact::redact_pdf_bytes(&pdf, &[redact::RedactionRegion {
    page: 0, x: 100.0, y: 700.0, width: 200.0, height: 20.0,
}])?;

// Rasterize page 0 to a PNG (pure Rust, no Ghostscript).
raster::rasterize_page(&pdf, 0, 96)?.write_png("page.png")?;

// Full SVG document → PDF.
let svg = std::fs::read_to_string("drawing.svg")?;
let pdf_bytes = pdfrs::vector::svg_document_to_pdf_bytes(
    &svg,
    pdfrs::pdf_generator::PageLayout::portrait(),
)?;
```

## Browser (WASM)

```bash
./scripts/build-wasm.sh
python3 -m http.server 8080
# open http://localhost:8080/wasm/example.html
```

Details: [wasm/README.md](wasm/README.md).

## Architecture

This tool is built with a modular architecture:

- **PDF Parser** (`src/pdf.rs`): PDF parsing, text extraction, validation, xref/object stream parsing
- **PDF Generator** (`src/pdf_generator.rs`): Creates PDFs with layout, color, alignment, accessibility, syntect highlighting
- **Elements** (`src/elements.rs`): 27 structured element types and markdown parser
- **Markdown** (`src/markdown.rs`): Markdown-to-PDF pipeline with rich formatting
- **PDF → Markdown** (`src/pdf_to_md.rs`): Structured reconstruction (headings, lists, code blocks)
- **Charts / thesis** (`src/chart.rs`, `src/thesis.rs`): Vector charts; TOC, folios, citations
- **Search** (`src/search.rs`): Full-text search with bounding boxes; shared content-stream walker
- **Redact** (`src/redact.rs`): True content-stream redaction (text removal + black overlay)
- **Rasterizer** (`src/raster.rs`): Pure-Rust PDF → PNG with inline PNG encoder
- **PDF Operations** (`src/pdf_ops.rs`): Merge, split, rotate, reorder, watermark, metadata, annotations
- **Image Handler** (`src/image.rs`): JPEG/PNG/BMP embedding with dimension parsing
- **Linearize / incremental** (`src/linearize.rs`, `src/incremental.rs`): Fast Web View; append-only updates
- **Plugins** (`src/plugin.rs`): Parser/generator hooks (e.g. callouts)
- **Compression** (`src/compression.rs`): PDF stream compression (deflate)
- **Security** (`src/security.rs`): Password protection, permissions (stub crypto gated)

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed module documentation. Spec and backlog: [SPEC.md](SPEC.md), [TODO.md](TODO.md).

## Testing

~395 tests (`cargo test`), including:
- **289 lib tests**: Unit tests across all modules
- **Integration crates**: `tests/integration.rs`, `capabilities_v2` (v0.2 features), `comprehensive_pdf`, `roundtrip_test`, `capability_validation`, `unicode_integration_test`
- **~35 doctests**: Public API examples

Round-trip validation tests verify that content survives: generate → validate → parse → verify. The `capabilities_v2` suite exercises rasterize → search → redact end-to-end plus full SVG rendering and structured PDF → Markdown.

```bash
cargo test
```

## Documentation

Project docs live at the repository root:

| File | Purpose |
|------|---------|
| [README.md](README.md) | Quick start, CLI usage, features |
| [CHANGELOG.md](CHANGELOG.md) | Versioned release notes |
| [SPEC.md](SPEC.md) | Functional / non-functional requirements |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Modules, data flow, design decisions |
| [TODO.md](TODO.md) | Backlog, audit follow-ups, brainstorming |

Extra material (contributing, validation notes) is under [`docs/`](docs/).

## Limitations

- **Rasterizer is schematic**: text glyphs render as gray rectangles sized to their advance width — useful for layout preview, not pixel-perfect typography. Use PDFium or Ghostscript for that.
- **Redaction is text-granular**: when a `Tj` string's bbox intersects a redacted region, the whole string is masked. Image XObjects under a redacted region are obscured by the overlay but not removed from the file.
- **PDF → Markdown** reconstruction is heuristic; very dense tables and multi-column layouts may not round-trip perfectly.
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
