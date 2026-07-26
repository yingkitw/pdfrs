# pdfrs Architecture Documentation

## Overview

**pdfrs** is architected as a modular system with clear separation of concerns. The design prioritizes maintainability, extensibility, and performance while implementing PDF functionality from scratch without external PDF libraries. The crate exposes a library API (`pdfrs`) and a CLI binary (`pdfcli`).

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     pdfcli (CLI / WASM)                     │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   PDF I/O   │  │ Markdown I/O│  │   Image / Chart I/O │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  PDF Core   │  │  Elements / │  │  Thesis / Plugins  │  │
│  │  Engine     │  │  Markdown   │  │  Linearize / Incr. │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │Compression  │  │  Security   │  │  Optimization /    │  │
│  │   Module    │  │  / i18n     │  │  Streaming / Parallel│ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Module Architecture

### 1. CLI Module (`src/main.rs`)

**Purpose**: Command-line interface and application orchestration

**Responsibilities**:

- Parse command-line arguments using `clap` (including global `--lang`)
- Route commands to appropriate handlers
- Coordinate between modules
- Handle application-level error reporting (optionally localized via `i18n`)

**Key commands** (non-exhaustive): `create`, `md-to-pdf`, `html-to-pdf`, `pdf-to-md`, `extract`, `merge`, `split`, `rotate`, `add-image`, `filter-image`, `draw-vector`, `draw-svg`, `embed-3d`, `generate-comprehensive`, `linearize-pdf`, `incremental-update`, `optimize-pdf`, `validate`, …
### 2. PDF Core Engine (`src/pdf.rs`)

**Purpose**: PDF parsing and text extraction

**Architecture Pattern**: Document Object Model (DOM) parser

**Key Classes**:

```rust
pub struct PdfDocument {
    pub version: String,
    pub objects: HashMap<u32, PdfObject>,
    pub catalog: u32,
    pub pages: Vec<u32>,
}

pub enum PdfObject {
    Dictionary(HashMap<String, PdfValue>),
    Stream { dictionary: HashMap<String, PdfValue>, data: Vec<u8> },
    Array(Vec<PdfValue>),
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
    Reference(u32, u32),
    Name(String),
}
```

**Parsing Pipeline**:

```
PDF File → Header Parser → XRef Parser → Object Parser → Document Builder → Text Extractor
```

**Design Decisions**:

- Lazy loading of objects for memory efficiency
- Simple object model for maintainability
- Stream-based processing for large files
- PDF 1.5+ support via `parse_xref_stream` (cross-reference streams) and `parse_object_stream` (`/Type /ObjStm` compressed objects)

### 3. PDF Generator (`src/pdf_generator/`)

**Purpose**: Create PDF files from scratch

**Architecture Pattern**: Builder Pattern

**Internal Submodules**:

- `src/pdf_generator/mod.rs` — `PdfGenerator` object model, PDF assembly, font resources, outline tree, tagged PDF, public API entry points
- `src/pdf_generator/content_stream.rs` — `ContentStreamBuilder` (cursor, page breaks, font switches), element-to-stream rendering, `prepare_elements_for_render`, `render_elements_to_builder`
- `src/pdf_generator/layout.rs` — `PageLayout`, `PageOrientation`, `PdfVersion`, `Color`, `TextAlign`, font-size / text-width helpers
- `src/pdf_generator/code_highlight.rs` — syntect-based syntax highlighting for code blocks
- `src/pdf_generator/math_layout.rs` — math layout helpers
- `src/pdf_generator/text_support.rs` — math-to-text conversion and PDF text encoding helpers
- `src/pdf_generator/unicode_support.rs` — Unicode TTF discovery/loading and glyph-ID encoder for embedded Type0/CIDFont paths
- `src/pdf_generator/accessibility.rs` — tagged PDF structure types / `AccessibilityOptions`

**Key Classes**:

```rust
pub struct PdfGenerator {
    objects: Vec<PdfObject>,
    next_id: u32,
}

struct PdfObject {
    id: u32,
    generation: u32,
    content: String,
    is_stream: bool,
    stream_data: Option<Vec<u8>>,
}
```

**Generation Pipeline**:

```
Text Input → Page Builder → Content Stream Generator → Object Manager → File Writer
```

**Design Decisions**:

- Object ID management for proper references
- Content stream optimization
- PDF compliance with version 1.4 specification

### 4. Structured Elements (`src/elements.rs`)

**Purpose**: Define and parse structured document elements from Markdown

**Architecture Pattern**: Line Scanner → Element Classifier → Element Tree

**Key Types**:

```rust
pub enum Element {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    RichParagraph { segments: Vec<TextSegment> },
    UnorderedListItem { text: String, depth: u8 },
    OrderedListItem { number: u32, text: String, depth: u8 },
    TaskListItem { checked: bool, text: String },
    CodeBlock { language: String, code: String },
    InlineCode { code: String },
    TableRow { cells: Vec<String>, is_separator: bool, alignments: Vec<TableAlignment> },
    BlockQuote { text: String, depth: u8 },
    DefinitionItem { term: String, definition: String },
    Footnote { label: String, text: String },
    Link { text: String, url: String },
    Image { alt: String, path: String },
    StyledText { text: String, bold: bool, italic: bool },
    MathBlock { expression: String },
    MathInline { expression: String },
    PageBreak,
    HorizontalRule,
    EmptyLine,
    Columns { count: u8 },
    Chart { kind: ChartKind, title: Option<String>, points: Vec<(String, f32)> },
    PageNumberMode { style: PageNumberStyle },
    RunningHeaderMode { enabled: bool },
    Toc,
    Bibliography,
    CitationDef { key: String, text: String },
}
```

(`Element` has 27 variants.)

**Key Functions**:

- `parse_markdown()`: Parse markdown text into `Vec<Element>`
- Supports headings, paragraphs, lists, tables, code, math, images, charts (` ```chart` `), columns, thesis directives (`toc`, page numbers, citations), etc.
- `strip_inline_formatting()`: Remove bold/italic/code/link/strikethrough syntax

**Design Decisions**:

- Elements carry formatting intent (heading level, list depth, checked state)
- Inline formatting stripped at parse time; structure preserved for PDF rendering
- Enables font size variations, indentation, and page numbering in PDF output

### 5. Markdown Converter (`src/markdown.rs`)

**Purpose**: Orchestrate Markdown-to-PDF and Markdown-to-text conversion

**Architecture Pattern**: Pipeline Coordinator

**Parsing Pipeline**:

```
Markdown Text → elements::parse_markdown() → Vec<Element> → pdf_generator::create_pdf_from_elements()
```

**Key Functions**:

- `markdown_to_text()`: Convert Markdown to plain text (legacy, uses elements internally)
- `markdown_to_pdf_with_options()`: Convert with styling via structured elements
- `elements_to_text()`: Render elements back to plain text

### 5b. HTML Converter (`src/html.rs`)

**Purpose**: Parse HTML documents and convert to PDF via the existing Element pipeline

**Architecture**: Lightweight HTML tokenizer → simplified DOM tree (`Node`) → `Element` vector → existing `pdf_generator` renderer

**Supported HTML elements**: `<h1>`–`<h6>`, `<p>`, `<strong>`/`<b>`, `<em>`/`<i>`, `<code>`, `<pre><code>`, `<ul>`/`<ol>`/`<li>`, `<table>`/`<thead>`/`<tbody>`/`<tr>`/`<th>`/`<td>`, `<blockquote>`, `<img>`, `<a>`, `<hr>`, `<br>`, `<s>`/`<del>`, `<div>`/`<section>`/`<article>`, `<span>`, HTML entities

**Key Functions**:

- `parse_html()`: Parse HTML string into `Vec<Element>`
- `html_to_pdf()`: Convert HTML to PDF file
- `html_to_pdf_bytes()`: Convert HTML to PDF bytes in memory

**CLI**: `html-to-pdf` command with `--font`, `--font-size`, `--landscape`, `--rtl`, `--columns`, `--profile` options

### 6. Image Processing (`src/image.rs`)

**Purpose**: Handle image embedding in PDFs

**Architecture Pattern**: Strategy Pattern for different image formats

**Supported Formats**:

- JPEG (with DCTDecode)
- PNG / BMP (decoded to RGB + FlateDecode)
- Markdown `![alt](path)` resolves relative to the markdown file (or `image_base_dir`); missing/unloadable images fail generation

### 7. Charts (`src/chart.rs`)

Vector bar / line / pie charts from fenced Markdown ` ```chart` ` blocks → `Element::Chart`.

### 8. Thesis layout (`src/thesis.rs`)

Roman/Arabic/hidden folios, running headers, TOC expansion, citation registry / bibliography.

### 9. Comprehensive sample (`src/comprehensive.rs`)

Bundled multi-feature document used by `generate-comprehensive` and `tests/comprehensive_pdf.rs`.

### 10. PDF Operations (`src/pdf_ops/`)

**Purpose**: High-level PDF manipulation operations, split into domain submodules.

**Module Structure**:

- `mod.rs`: Core operations — `merge_pdfs`, `split_pdf`, `rotate_pdf`, `reorder_pages`, `watermark_pdf`, `overlay_image_on_pdf`, `create_pdf_with_images`, `extract_images_from_pdf`; shared helpers (`extract_page_streams`, `build_page_streams`, `escape_pdf_meta`, `extract_pdf_dict_value`)
- `metadata.rs`: `PdfMetadata` struct, `create_pdf_with_metadata`, `extract_metadata_from_pdf`, `merge_metadata`, `assemble_pdf_with_metadata`
- `annotations.rs`: `TextAnnotation`, `LinkAnnotation`, `HighlightAnnotation`, `ThreeDAnnotation`, `create_pdf_with_annotations`, `create_pdf_with_3d_annotation`
- `forms.rs`: `FormField`, `FormFieldType`, `DetectedFormField`, `create_pdf_with_form_fields`, `detect_form_fields`, `fill_form_fields`
- `security.rs`: `protect_pdf`, `sign_pdf`, `verify_pdf_signature`, `SignatureInfo`, `extract_certificates_from_pdf`
- `tables.rs`: `extract_tables_from_pdf` (heuristic CSV extraction from content streams)
- `structure.rs`: `detect_document_structure`, `DocumentStructure`, `DetectedHeading`, `DetectedSection`
- `portfolio.rs`: `create_portfolio_pdf` (PDF Collection with embedded files)

**Key Types**:

- `PdfMetadata`: Document properties (title, author, subject, keywords, creator)
- `TextAnnotation`: Positioned text note on a page
- `LinkAnnotation`: Clickable URI region on a page
- `HighlightAnnotation`: Colored highlight rectangle with QuadPoints
- `ThreeDAnnotation`: U3D 3D annotation rectangle + activation (`/3DA`)
- `FormField` / `FormFieldType`: Interactive AcroForm fields
- `SignatureInfo`: Detected digital signature metadata
- `DocumentStructure`: Headings, sections, and body font size detected from PDF
- `WatermarkType` / `WatermarkPosition`: Watermark styling and placement

### 11. Internationalization (`src/i18n.rs`)

**Purpose**: Localized CLI/validation messages and number formatting

**Key Types / Functions**:

- `Locale`: `en` / `es` / `de` / `fr` / `zh` / `he` / `ar` (from `--lang`, `PDFRS_LANG`, or `LANG`)
- `MsgId` + `t` / `tf`: message catalog with `{0}` placeholders
- `localize_validation()`: translate known structural validation strings
- `format_integer` / `format_decimal`: locale-aware separators

### 12. Plugin System (`src/plugin.rs`)

**Purpose**: Extensible Markdown parsing and element transforms

**Key Types**:

- `ParserPlugin` / `GeneratorPlugin` traits
- `PluginRegistry` — register and run plugins
- `CalloutPlugin` — `:::note` / `:::warning` / … fenced callouts
- `parse_markdown_with_plugins()` — parse + transform pipeline

Document bookmarks (`/Outlines`) are produced automatically from headings during PDF assembly in `pdf_generator`.

### 13. Linearization (`src/linearize.rs`)

**Purpose**: Fast Web View / progressive PDF loading

**Key Functions**:

- `linearize_pdf_bytes` / `linearize_pdf_file` — rewrite with `/Linearized` dict + first-page object priority
- `is_linearized` — detect Fast Web View structure
- Wired into `optimize_pdf_bytes` when `OptimizationSettings.linearize` is true (Web/Ebook profiles)

### 14. Incremental Updates (`src/incremental.rs`)

**Purpose**: Append-only PDF saves without rewriting prior bytes

**Key Functions**:

- `incremental_append_objects` — low-level object + xref/`/Prev` trailer append
- `incremental_set_info` — update title/author via new `/Info`
- `incremental_add_text_annotation` — append a text note + catalog override
- `is_incremental_pdf` — detect multiple `%%EOF` markers

### 15. Streaming / optimization / vector / security

- `src/streaming.rs` — memory-efficient streaming generation
- `src/optimization.rs` — web/print/archive/ebook profiles
- `src/vector.rs` — vector paths, SVG `d` import, **and full SVG document rendering** (`<g transform>`, `<rect>`, `<circle>`, `<ellipse>`, `<line>`, `<polyline>`, `<polygon>`, `<path>`, `<text>`)
- `src/security.rs` — permissions; protected encrypt/decrypt returns `Err` until real crypto lands

### 15b. Rasterization (`src/raster.rs`)

**Purpose**: Pure-Rust PDF page rasterization (PDF → PNG) with no external renderer dependency.

**Key Functions**:

- `rasterize_page(pdf_bytes, page_index, dpi) -> RasterPage`
- `rasterize_all(pdf_bytes, dpi) -> Vec<RasterPage>`
- `RasterPage::to_png() -> Vec<u8>` (inline PNG encoder: signature + IHDR + zlib IDAT via `flate2` + IEND, built-in CRC-32)

**Scope**: renders the operators emitted by `pdfrs` plus the common content-stream subset from other producers (`q`/`Q`, `cm`, color ops, path construction, path painting, `BT`/`ET`, `Tf`, text-positioning ops, `Tj`/`TJ`). Text is rendered as **gray glyph-block rectangles** sized to advance widths (schematic rasterizer — not pixel-perfect typography).

### 15c. Search (`src/search.rs`)

**Purpose**: Full-text search with per-hit bounding boxes. Also the **shared content-stream helpers hub** used by `raster`, `redact`, and `pdf_to_md`.

**Key Functions / Types**:

- `search_text(pdf_bytes, query, case_insensitive) -> Vec<SearchHit>`
- `SearchHit { page, text, snippet, bbox: Rect }`
- `Rect { x, y, width, height }` with `intersects` / `contains` helpers
- `pub(crate)` helpers: `collect_pages_from_doc` (raw-buffer-aware page walker), `collect_font_metrics`, `tokenize`, `extract_string`, `extract_tj_array`, `extract_font_name`, `decompress_stream`, `as_ref_id`, `parse_kids_string`, `raw_kids_for_object`

**Design note**: the bundled `parse_dict_entries` in `pdf.rs` is whitespace-token-only and truncates `/Kids [a b c]` to `[a`; `collect_pages_from_doc` accepts an optional raw-bytes slice and falls back to `raw_kids_for_object` to recover the full array.

### 15d. Redaction (`src/redact.rs`)

**Purpose**: True content-stream redaction — rewrites streams to mask text instead of relying on opaque overlays.

**Key Functions / Types**:

- `redact_pdf_bytes(pdf_bytes, &[RedactionRegion]) -> Vec<u8>`
- `redact_pdf_bytes_with_style(pdf_bytes, regions, RedactionStyle::Strip | BlackBox)`
- `RedactionRegion { page, x, y, width, height }`

**Behaviour**: walks each page's content stream, computes the bounding box of each `Tj`/`TJ`, and replaces intersecting text with whitespace-equivalent masks. `BlackBox` style additionally appends a solid-black filled rectangle over each region. Stream compression is preserved (FlateDecode streams are recompressed after rewriting).

### 15e. PDF → Markdown (`src/pdf_to_md.rs`)

**Purpose**: Structured PDF → Markdown reconstruction (replaces the plain-text dump produced by `pdf::extract_text`).

**Key Functions**:

- `pdf_to_markdown_bytes(pdf_bytes) -> String`
- `pdf_to_markdown_file(input_pdf, output_md)`

**Heuristics**: body font size detected by character-count-weighted mode; heading levels 1-5 inferred from `line.max_font_size / body_size` ratios; bullets, numbered lists, code blocks (Courier detection), and horizontal rules reconstructed; ToUnicode-aware decoding of CID-font glyph-ID hex strings; spaces inserted between adjacent Tj spans on the same line using a 0.4-em-per-char width estimate.

### 16. PDF Validation (`src/pdf/validation.rs`)

**Purpose**: Validate PDF structural integrity and compliance

Submodule of the `pdf` module. Public items are re-exported at `crate::pdf::`,
so callers continue to use `crate::pdf::validate_pdf_bytes` etc.

**Key Functions**:

- `validate_pdf()`: Validate a PDF file on disk
- `validate_pdf_bytes()`: Validate PDF bytes in memory (no filesystem needed)

**Compliance Checks**:

- `validate_pdf_a_bytes()` / `validate_pdf_a3_bytes()`: PDF/A-1b and PDF/A-3b
- `validate_pdf_ua_bytes()`: PDF/UA-1 accessibility
- `check_screen_reader_compliance_bytes()`: PDF/UA + text-extraction audit

**Key Types**:

```rust
pub struct PdfValidation {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub page_count: usize,
    pub object_count: usize,
}
```

**Validation Checks**: PDF header, %%EOF marker, xref table, trailer, /Catalog, /Pages, page count, object/endobj pairing, stream/endstream pairing, /Root reference.

### 17. Compression Module (`src/compression.rs`)

**Purpose**: Handle PDF stream compression

**Architecture Pattern**: Strategy Pattern for compression algorithms

**Key Functions**:

- `decompress_deflate()`: Decompress zlib/deflate streams
- `compress_deflate()`: Compress data streams
- `decode_hex_string()`: Decode hex-encoded strings
- `encode_hex_string()`: Encode data as hex strings

## Data Flow Architecture

### PDF Generation Flow

```
Input Text → Markdown Parser → Content Builder → PDF Generator → File Writer
     ↓              ↓              ↓                ↓             ↓
Raw String → Structured Text → Content Streams → PDF Objects → Binary File
```

### PDF Parsing Flow

```
PDF File → Header Parser → Object Locator → Object Parser → Document Builder → Text Extractor
    ↓           ↓            ↓              ↓              ↓             ↓
Binary File → PDF Version → Object Offsets → PDF Objects → Document Model → Plain Text
```

### Conversion Flow

```
Source File → Appropriate Parser → Text Processing → Target Generator → Output File
     ↓              ↓                  ↓                ↓               ↓
PDF/MD File → PDF/MD Parser → Text Transformation → Target Generator → Target Format
```

## Design Patterns Used

### 1. Builder Pattern

- **Location**: `PdfGenerator`
- **Purpose**: Construct complex PDF objects step by step
- **Benefits**: Fluent interface, flexible configuration

### 2. Strategy Pattern

- **Location**: `Image` and `Compression` modules
- **Purpose**: Handle different formats and algorithms
- **Benefits**: Extensibility, interchangeable algorithms

### 3. Command Pattern

- **Location**: CLI module
- **Purpose**: Encapsulate user commands as objects
- **Benefits**: Decoupling, undo/redo capabilities

### 4. Iterator Pattern

- **Location**: PDF parsing
- **Purpose**: Traverse PDF objects and collections
- **Benefits**: Uniform interface, lazy evaluation

## Performance Considerations

### Memory Management

- **Strategy**: Streaming for large files, lazy loading
- **Implementation**: Buffered readers, object pools
- **Benefits**: Reduced memory footprint, better scalability

### CPU Optimization

- **Strategy**: Efficient string handling, minimal allocations
- **Implementation**: String builders, buffer reuse
- **Benefits**: Faster processing, lower CPU usage

### I/O Optimization

- **Strategy**: Buffered I/O, batch operations
- **Implementation**: Buffered readers/writers, bulk operations
- **Benefits**: Fewer system calls, better throughput

## Error Handling Architecture

### Error Hierarchy

```
Error
├── ParseError (PDF parsing failures)
├── IoError (File system issues)
├── FormatError (Unsupported formats)
└── ValidationError (Invalid input)
```

### Error Propagation

- **Strategy**: Result<T, Error> throughout the codebase
- **Implementation**: Question mark operator (?) for propagation
- **Benefits**: Explicit error handling, clear error paths

### Recovery Mechanisms

- **Partial Processing**: Continue processing other objects on failure
- **Graceful Degradation**: Fallback to simpler processing modes
- **User Feedback**: Clear error messages and suggestions

## Testing Architecture

### Test Organization

```
tests/
├── integration.rs
├── comprehensive_pdf.rs
├── roundtrip_test.rs
├── capability_validation.rs
├── capabilities_v2.rs             # v0.2 features: rasterize/search/redact/SVG/PDF→MD
├── unicode_integration_test.rs
└── fixtures/          # markdown, sample.png, certs
benches/
└── pdf_benchmarks.rs
```

### Test Strategies

- **Unit Tests**: Individual module functionality (`cargo test --lib`)
- **Integration Tests**: CLI and library end-to-end flows
- **Property Tests**: Input validation and edge cases (`proptest`)
- **Doctests**: Public API examples in module docs

## Extensibility Design

### Plugin Architecture

```
Plugin Interface
├── ParserPlugin (Markdown → Elements)
├── GeneratorPlugin (Elements → PDF hooks)
└── CalloutPlugin (shipped default)
```

Shipped: `PluginRegistry`, `--plugins callouts`, `PdfBuilder::add_markdown_with_plugins`.

### Configuration System (Future)

- File-based configuration
- Runtime parameter adjustment
- Feature toggles
- Performance tuning options

## Security Architecture

### Input Validation

- File type verification
- Size limitations
- Content sanitization
- Path traversal prevention

### Resource Management

- Memory limits
- File handle limits
- Processing timeouts
- Temporary file cleanup

## Future Architecture Enhancements

### Multi-threading Support

- Parallel PDF parsing
- Concurrent image processing
- Background I/O operations
- Worker thread pools

### Caching System

- Object caching for repeated operations
- Result memoization
- Temporary file caching
- Metadata caching

### Plugin System

- Dynamic loading of parsers
- Custom output generators
- Extensible filter system
- Third-party integrations

This architecture provides a solid foundation for the current implementation while allowing for future enhancements and maintainability.
