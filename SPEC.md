# pdfrs Technical Specification

## Overview

**pdfrs** is a Rust library and CLI (`pdfcli`) for reading, writing, and converting PDF files to and from Markdown. The implementation is self-contained (no external PDF libraries) and implements core PDF specifications from scratch.

## Requirements

### Functional Requirements

#### FR1: PDF Generation

- **FR1.1**: Create PDF files from raw text input
- **FR1.2**: Support customizable fonts (Helvetica, Times-Roman, Courier)
- **FR1.3**: Support customizable font sizes
- **FR1.4**: Automatically split content into multiple pages when needed
- **FR1.5**: Generate PDFs compliant with PDF 1.4 specification

#### FR2: PDF Parsing

- **FR2.1**: Parse PDF file structure and extract objects
- **FR2.2**: Extract text content from PDF pages
- **FR2.3**: Handle compressed streams (deflate/zlib)
- **FR2.4**: Process PDF content streams and text operators
- **FR2.5**: Detect and handle different PDF encodings

#### FR3: Markdown Integration

- **FR3.1**: Parse Markdown syntax (headers, lists, emphasis, code blocks, tables)
- **FR3.2**: Convert Markdown to structured elements for rich PDF generation
- **FR3.3**: Convert extracted PDF text to Markdown format
- **FR3.4**: Preserve document structure during conversions
- **FR3.5**: Task list support (`- [x]` / `- [ ]`)
- **FR3.6**: Strikethrough text (`~~text~~`)
- **FR3.7**: Blockquote support with nesting (`>`, `>>`, `>>>`)
- **FR3.8**: Definition lists (`term` / `: definition`)
- **FR3.9**: Table alignment parsing (`:---`, `:---:`, `---:`)
- **FR3.10**: Inline math parsing inside regular paragraphs (`$...$` in mixed text lines)

#### FR4: Image Support

- **FR4.1**: Detect image formats (JPEG, PNG, BMP) with dimension parsing
- **FR4.2**: Embed JPEG images in PDF files (DCTDecode)
- **FR4.3**: Support image positioning and sizing with aspect-ratio scaling
- **FR4.4**: CLI `add-image` command fully wired
- **FR4.5**: Image filters and effects — grayscale, invert, brightness, contrast, sepia on BMP/PNG; `filter-image` CLI
- **FR4.6**: Vector graphics — lines, rectangles, ellipses, polygons, cubic Bézier paths via `VectorCanvas`; `draw-vector` CLI
- **FR4.7**: SVG path import — parse SVG `d` attributes (`M/L/H/V/C/S/Q/T/Z`) into PDF paths; `draw-svg` CLI
- **FR4.8**: Markdown image embedding — `![alt](path)` loads JPEG/PNG/BMP, scales to content width, and registers `/XObject` resources; missing/unloadable images fail generation
- **FR4.9**: Markdown charts — fenced ` ```chart bar|line|pie` ` blocks render vector bar, line, and pie charts
- **FR4.10**: Academic thesis layout — Roman/Arabic/hidden folios, running headers, in-document TOC, numbered figure/table captions, `[@cite]` citations + bibliography

#### FR5: CLI Interface

- **FR5.1**: Provide subcommands for different operations
- **FR5.2**: Support command-line arguments for customization
- **FR5.3**: Provide helpful error messages and usage information
- **FR5.4**: Support input/output file specifications
- **FR5.5**: Page orientation (`--landscape` flag)

#### FR6: PDF Generation Enhancements

- **FR6.1**: Header font size hierarchy (H1=2x, H2=1.6x, H3=1.3x, H4=1.1x)
- **FR6.2**: Page numbering in footer
- **FR6.3**: Code block rendering with reduced font size (0.85x)
- **FR6.4**: Horizontal rule rendering
- **FR6.5**: Configurable page layout (portrait/landscape; optional RTL via `PageLayout::with_rtl` / `md-to-pdf --rtl`)
- **FR6.6**: Structured element pipeline (Markdown → Elements → PDF)
- **FR6.7**: Unicode-aware line wrapping and width estimation (ASCII/CJK/emoji-aware)
- **FR6.8**: Unicode-safe text emission for non-ASCII text in line/code/math rendering (UTF-16BE for Base-14 path; glyph-ID CID encoding for embedded Type0/CIDFont path)
- **FR6.9**: Embed Unicode-capable TrueType font as Type0/CIDFont (`FontFile2`) for cross-viewer glyph reliability
- **FR6.10**: Allow unicode font override via `PDFRS_UNICODE_FONT_PATH`
- **FR6.11**: Ensure math rendering paths (including italic math styling) use glyph-safe Unicode encoding when embedded Type0/CIDFont mode is active
- **FR6.12**: Localized validation/CLI messages (`i18n` module; `--lang` / `PDFRS_LANG`; en/es/de/fr/zh/he/ar) and locale number formatting
- **FR6.13**: Multi-column layout — `PageLayout::{columns,column_gap,with_columns}`, Markdown `<!-- columns:N -->`, CLI `--columns`; H1 stays full-width across columns

#### FR7: PDF Manipulation

- **FR7.1**: Merge multiple PDFs into a single output (`merge` command)
- **FR7.2**: Split PDF by page range (`split` command)
- **FR7.3**: Rotate all pages by 0/90/180/270° (`rotate` command)
- **FR7.4**: Document metadata embedding (title, author, subject, keywords) (`md-to-pdf-meta`)

#### FR8: Annotations and Multi-Image

- **FR8.1**: Text annotations with positioned notes on pages
- **FR8.2**: Link annotations with clickable URI actions
- **FR8.3**: Multiple JPEG images per page with independent positioning
- **FR8.4**: Highlight annotations with QuadPoints and color

#### FR9: Library API

- **FR9.1**: In-memory PDF generation via `generate_pdf_bytes()` (no filesystem needed)
- **FR9.2**: PDF structural validation via `validate_pdf_bytes()` returning `PdfValidation`
- **FR9.3**: Rich `Element` enum with 27 variants for document modeling
- **FR9.4**: Round-trip validation: generate → validate → parse → verify content
- **FR9.5**: Cross-reference stream parsing for PDF 1.5+ (`parse_xref_stream`)
- **FR9.6**: Object stream handling for compressed objects (`parse_object_stream`)

#### FR10: Extended Markdown Elements

- **FR10.1**: Image elements (`![alt](path)`) parsed and embedded as PDF XObjects (JPEG/PNG/BMP)
- **FR10.1b**: Chart elements from fenced ` ```chart` ` blocks (bar / line / pie)
- **FR10.2**: Standalone link elements (`[text](url)`) parsed and rendered in blue
- **FR10.3**: Page break elements (`<!-- pagebreak -->` or `\pagebreak`)
- **FR10.4**: Inline code elements rendered with gray color
- **FR10.5**: Styled text elements (bold/italic) preserved
- **FR10.6**: Footnotes with label and text (`[^label]: text`)
- **FR10.7**: Definition lists (`term` / `: definition`)

#### FR11: Text Styling

- **FR11.1**: RGB color support via `Color` struct
- **FR11.2**: Text alignment (Left, Center) via `TextAlign` enum
- **FR11.3**: H1 headings centered, code blocks in gray, links in blue
- **FR11.4**: Watermarks with diagonal text, configurable opacity/size

#### FR20: Native Page Rasterization (PDF → PNG)

- **FR20.1**: Pure-Rust page rasterizer with no external PDF/font dependencies — `src/raster.rs`, `raster::rasterize_page`, `raster::rasterize_all`
- **FR20.2**: Inline PNG encoder (signature, IHDR, zlib-compressed IDAT via `flate2`, IEND) with built-in CRC-32
- **FR20.3**: Renders PDF content-stream operators emitted by `pdfrs` plus common operators from other producers: `q`/`Q`, `cm`, `w`, `rg`/`RG`/`g`/`G`/`k`/`K`, `m`/`l`/`c`/`h`/`re`, `S`/`s`/`f`/`F`/`B`/`b`/`n`, `BT`/`ET`, `Tf`, `Tm`/`Td`/`TD`/`T*`, `Tj`/`TJ`/`'`/`"`
- **FR20.4**: Base-14 PDF font width tables (Helvetica, Times-Roman, Courier) and `/W` array handling for CIDFont
- **FR20.5**: Page size honoured via the page's `/MediaBox`; DPI parameter scales pixel dimensions
- **FR20.6**: CLI command `rasterize-pdf` (single page → PNG file or all pages → directory)
- **FR20.7**: Text is rendered as gray glyph-block rectangles sized to advance widths (schematic rasterizer — not pixel-perfect typography)

#### FR21: Full-Text Search with Bounding Boxes

- **FR21.1**: `search::search_text(pdf_bytes, query, case_insensitive) -> Vec<SearchHit>` returning page, matched text, snippet, and bounding rectangle per hit
- **FR21.2**: Bounding box computed from font-size × advance width per character
- **FR21.3**: Case-insensitive matching folds both needle and haystack to lowercase
- **FR21.4**: Snippet generation includes ~24 characters of context on each side with ellipses when truncated
- **FR21.5**: `Rect::intersects` and `Rect::contains` helpers for downstream consumers (redaction, viewer integration)
- **FR21.6**: CLI command `search-pdf` with optional `--json` output for programmatic consumption

#### FR22: True Content-Stream Redaction

- **FR22.1**: `redact::redact_pdf_bytes(pdf_bytes, &[RedactionRegion])` returns redacted PDF bytes with content streams rewritten
- **FR22.2**: `RedactionStyle::BlackBox` (default) replaces intersecting text with whitespace-equivalent mask AND appends a solid-black filled rectangle over each region
- **FR22.3**: `RedactionStyle::Strip` replaces intersecting text without the black overlay
- **FR22.4**: Stream compression is preserved (FlateDecode-compressed streams are recompressed after rewriting)
- **FR22.5**: Redaction operates at Tj granularity — when a Tj's bounding box intersects a region, the entire string is masked (partial-string redaction is not supported)
- **FR22.6**: CLI command `redact-pdf` accepts repeatable `--region page,x,y,w,h` arguments

#### FR23: Full SVG Document Rendering

- **FR23.1**: `vector::parse_svg_document(svg, layout) -> VectorCanvas` parses a full SVG document with an inline minimal XML parser
- **FR23.2**: Supported elements: `<svg>`, `<g>`, `<rect>`, `<circle>`, `<ellipse>`, `<line>`, `<polyline>`, `<polygon>`, `<path d="...">`, `<text>`, `<defs>`, `<symbol>`, `<tspan>`
- **FR23.3**: Transform composition via `parse_svg_transform` supporting `translate`, `scale`, `rotate` (with optional centre), `matrix`, `skewX`, `skewY`
- **FR23.4**: Style attributes (`fill`, `stroke`, `stroke-width`, `opacity`) inherited through parent `<g>` via `with_style_scope`
- **FR23.5**: Paint parsing supports named colours, `#rgb` / `#rrggbb` hex, and `rgb(r,g,b)` syntax
- **FR23.6**: Y-axis flipped (`[1, 0, 0, -1, 0, layout.height]`) so SVG top-left origin maps to PDF bottom-left
- **FR23.7**: `vector::svg_document_to_pdf_bytes` and `svg_document_file_to_pdf` library APIs
- **FR23.8**: CLI command `draw-svg-file` renders a full SVG file to a one-page PDF
- **FR23.9**: Backwards compatible with existing `extract_svg_path_d` / `svg_path_to_pdf_bytes` single-path APIs

#### FR24: Structured PDF → Markdown Conversion

- **FR24.1**: `pdf_to_md::pdf_to_markdown_bytes` walks content streams, groups text spans into lines by Y proximity, emits Markdown
- **FR24.2**: Body font-size detected by character-count-weighted mode (so a one-word heading doesn't outvote paragraphs)
- **FR24.3**: Heading levels 1-5 inferred from `line.max_font_size / body_size` ratios (≥2.4→H1, ≥1.9→H2, ≥1.5→H3, ≥1.25→H4, else H5)
- **FR24.4**: Bullet lists detected from leading `•`, `- `, `* ` prefixes; numbered lists from `\d+[.)] ` prefixes
- **FR24.5**: Code blocks detected when consecutive lines use a monospace font (Courier family); wrapped in ` ``` ` fences
- **FR24.6**: Horizontal rules detected from runs of `-`, `_`, or `─` characters
- **FR24.7**: Spaces inserted between adjacent Tj spans on the same line using 0.4-em-per-char width estimate
- **FR24.8**: ToUnicode-aware decoding of CID-font glyph-ID hex strings via `pdf::collect_tounicode_gid_map` + `decode_pdf_hex_string_with_map`
- **FR24.9**: UTF-16BE BOM strings (`FE FF …`) decoded natively
- **FR24.10**: CLI command `pdf-to-md` upgraded to use the structured converter; falls back to plain `extract_text` on conversion errors

### Non-Functional Requirements

#### NFR1: Performance

- **NFR1.1**: Process small PDF files (<1MB) in under 1 second
- **NFR1.2**: Handle large text files without memory issues
- **NFR1.3**: Efficient memory usage during PDF generation

#### NFR2: Compatibility

- **NFR2.1**: Support PDF files created by common applications
- **NFR2.2**: Generate PDFs readable by standard PDF viewers
- **NFR2.3**: Support common Markdown syntax variants

#### NFR3: Reliability

- **NFR3.1**: Handle malformed PDF files gracefully
- **NFR3.2**: Provide clear error messages for troubleshooting
- **NFR3.3**: Not crash on unexpected input

## System Architecture

### Core Components

#### 1. PDF Parser Module (`src/pdf.rs`)

```
PdfDocument
├── version: String
├── objects: HashMap<u32, PdfObject>
├── catalog: u32
└── pages: Vec<u32>

PdfObject
├── Dictionary(HashMap<String, PdfValue>)
├── Stream { dictionary, data }
├── Array(Vec<PdfValue>)
├── String(String)
├── Number(f64)
├── Boolean(bool)
├── Null
├── Reference(u32, u32)
└── Name(String)
```

**Responsibilities:**

- Parse PDF file structure
- Extract objects from PDF streams
- Handle compressed data
- Process content streams for text extraction

#### 2. PDF Generator Module (`src/pdf_generator.rs`)

```
PdfGenerator
├── objects: Vec<PdfObject>
└── next_id: u32

PdfObject
├── id: u32
├── generation: u32
├── content: String
├── is_stream: bool
└── stream_data: Option<Vec<u8>>
```

**Responsibilities:**

- Create PDF file structure
- Generate content streams
- Handle font resources
- Create page tree and catalog
- Write valid PDF format

#### 3. Markdown Parser (`src/markdown.rs`)

```
MarkdownParser
├── headers: Vec<Header>
├── paragraphs: Vec<Paragraph>
├── lists: Vec<List>
├── tables: Vec<Table>
└── code_blocks: Vec<CodeBlock>
```

**Responsibilities:**

- Parse Markdown syntax
- Convert to plain text
- Handle formatting preservation
- Process tables and lists

#### 4. Image Handler (`src/image.rs`)

```
ImageHandler
├── format_detector: FormatDetector
├── jpeg_processor: JpegProcessor
├── png_processor: PngProcessor
└── bmp_processor: BmpProcessor
```

**Responsibilities:**

- Detect image formats
- Process image data
- Create PDF image objects
- Generate image content streams

#### 5. Compression Module (`src/compression.rs`)

```
CompressionHandler
├── deflate_compressor: DeflateCompressor
├── hex_encoder: HexEncoder
└── stream_processor: StreamProcessor
```

**Responsibilities:**

- Compress and decompress streams
- Handle hex encoding/decoding
- Process compressed PDF objects

### Data Flow

#### PDF Generation Flow

```
Text Input → Markdown Parser → Text Processor → PDF Generator → PDF File
```

#### PDF Parsing Flow

```
PDF File → PDF Parser → Object Extractor → Text Processor → Text Output
```

#### Markdown to PDF Flow

```
Markdown File → Markdown Parser → Text Processor → PDF Generator → PDF File
```

## Algorithms

### PDF Object Parsing

1. Read PDF header to determine version
2. Locate and parse xref table
3. Extract objects based on xref references
4. Parse object dictionaries and streams
5. Handle compressed streams if present
6. Build object graph for document structure

### Text Extraction Algorithm

1. Iterate through page objects
2. Extract content streams from pages
3. Decompress streams if necessary
4. Parse content stream operators
5. Extract text strings from operators
6. Apply positioning and formatting
7. Combine text from all pages

### PDF Generation Algorithm

1. Create page objects with content streams
2. Generate font resources
3. Create page tree structure
4. Generate document catalog
5. Calculate object offsets
6. Generate xref table
7. Write trailer and EOF marker

### Markdown Parsing Algorithm

1. Tokenize input into lines
2. Identify block elements (headers, lists, code blocks, tables)
3. Parse inline elements (emphasis, links, code)
4. Build document structure
5. Convert to plain text representation

## Error Handling

### Error Types

1. **Parse Errors**: Malformed PDF structure
2. **IO Errors**: File access issues
3. **Format Errors**: Unsupported content
4. **Encoding Errors**: Invalid character encodings

### Error Recovery

- Skip malformed objects when possible
- Provide partial results when complete parsing fails
- Generate warnings for non-critical issues
- Fail gracefully with helpful error messages

## Security Considerations

### Input Validation

- Validate PDF file structure
- Check for buffer overflows
- Validate image file formats
- Sanitize text content

### Resource Limits

- Limit maximum file size
- Limit number of objects processed
- Limit recursion depth in parsing
- Monitor memory usage

## Performance Considerations

### Optimization Strategies

- Stream-based processing for large files
- Lazy loading of PDF objects
- Efficient string handling
- Minimal memory allocations

### Benchmarks

- Target: <1s for 1MB PDF processing
- Target: <100MB memory usage for typical operations
- Target: 10MB/s text extraction rate

## Testing Strategy

### Unit Tests

- PDF object parsing
- Text extraction algorithms
- Markdown parsing
- Image format detection
- Compression functions

### Integration Tests

- End-to-end PDF generation
- PDF to Markdown conversion
- Markdown to PDF conversion
- CLI command functionality

### Performance Tests

- Large file processing
- Memory usage profiling
- CPU usage monitoring
- Concurrency testing

## Future Enhancements

### Completed Features

- Advanced PDF parsing (xref streams, object streams, font encodings)
- Annotations (text, link, highlight)
- PDF manipulation (merge, split, rotate, reorder, watermark)
- Security (password protection, permissions)
- Library API (in-memory generation, validation)
- 27 element types with round-trip validation
- ~395 tests (`cargo test`: 289 lib + integration crates + ~35 doctests)
- Charts, multi-column layout, thesis TOC/citations, linearized + incremental PDF
- WebAssembly (`wasm` feature) + canvas viewer demo
- Native PDF → PNG rasterizer (no external renderer dependency)
- Full-text search with per-hit bounding boxes
- True content-stream redaction (text removal, not overlay)
- Full SVG document rendering (`<g transform>`, shapes, text)
- Structured PDF → Markdown conversion (headings, lists, code blocks)

### Remaining Features

- Real PDF encryption (crypto currently gated to refuse fake protection)
- Full tagged PDF output for accessibility
- Pixel-perfect font-outline rasterization (current rasterizer is schematic — gray glyph blocks)
- Partial-string redaction (current implementation masks whole `Tj` spans)
- Image XObject removal during redaction (currently obscured by overlay only)
- Digital signature verification / richer signing UX
- Expanded rustdoc API examples

### Advanced Features (Surpassing Ghostscript)

#### FR12: Streaming & Incremental Processing

- **FR12.1**: Streaming PDF generation for large documents
- **FR12.2**: Page-by-page rendering without full document load
- **FR12.3**: Incremental PDF writing (stream to disk during generation)
- **FR12.4**: Memory-efficient processing of multi-gigabyte PDFs

#### FR13: Performance & Parallelism

- **FR13.1**: Parallel page processing using Rayon
- **FR13.2**: Concurrent PDF merging (process multiple files in parallel)
- **FR13.3**: SIMD-optimized text rendering operations
- **FR13.4**: Lazy loading of PDF pages (load only needed pages)
- **FR13.5**: Async PDF processing for web servers

#### FR14: Smart Content Analysis

- **FR14.1**: AI-powered structure detection (headers, sections, tables)
- **FR14.2**: Automatic table extraction to CSV/Excel
- **FR14.3**: Smart form field detection and filling
- **FR14.4**: Content-aware compression (compress low-importance images)
- **FR14.5**: Automatic PDF/A validation and conversion

#### FR15: Developer Experience Features

- **FR15.1**: Type-safe PDF builder API with compile-time guarantees
- **FR15.1b**: Plugin system — `ParserPlugin` / `GeneratorPlugin`, `PluginRegistry`, `CalloutPlugin`, `md-to-pdf --plugins`, `PdfBuilder::add_markdown_with_plugins`
- **FR15.1c**: Document outlines/bookmarks — headings emit `/Outlines` with `/PageMode /UseOutlines`
- **FR15.2**: Property-based testing for PDF generation
- **FR15.3**: Diff/patch support for PDF version control
- **FR15.4**: Hot-reload PDF preview during development
- **FR15.5**: Interactive REPL for PDF manipulation

#### FR16: WebAssembly & Browser Support

- **FR16.1**: Compile to WASM for browser-based PDF rendering — ✅ `wasm` feature, `wasm-pack` build
- **FR16.2**: JavaScript API for web applications — ✅ `render_markdown_to_pdf()` in `src/wasm.rs`
- **FR16.3**: JavaScript bindings and npm package — ✅ `wasm/package.json`, `scripts/build-wasm.sh`
- **FR16.4**: Canvas-based PDF viewer in browser — ✅ `wasm/viewer.js` + pdf.js preview in `wasm/example.html`
- **FR16.5**: Real-time collaborative PDF editing — planned

#### FR17: Advanced Format Support

- **FR17.1**: PDF 2.0 feature support
- **FR17.2**: PDF/A-3 and PDF/UA (universal accessibility) — includes `check_screen_reader_compliance_bytes()` and `check-screen-reader` CLI for PDF/UA + text-extraction validation
- **FR17.3**: Embedded attachments with metadata
- **FR17.4**: Portfolio and collection support
- **FR17.5**: 3D annotations and rich media — ✅ U3D via `ThreeDAnnotation` / `create_pdf_with_3d_annotation*`, `embed-3d` CLI (`/Subtype /3D` + `/Subtype /U3D` stream)

#### FR18: Intelligent Optimization

- **FR18.1**: Smart image compression based on content importance
- **FR18.2**: Font subsetting to reduce file size
- **FR18.3**: Object deduplication across pages
- **FR18.4**: Automatic optimization profiles (web, print, archive, ebook)
- **FR18.5**: Quality-aware compression (maintain visual quality)
- **FR18.6**: Linearized PDF / Fast Web View — ✅ `linearize_pdf_bytes`, `linearize-pdf` CLI, Web/Ebook optimize profiles
- **FR18.7**: Incremental PDF updates — ✅ append-only `/Prev` updates via `incremental` module + `incremental-update` CLI

#### FR19: Security & Validation

- **FR19.1**: Malformed PDF detection and sanitization
- **FR19.2**: JavaScript sandbox for PDF actions — ✅ `sandbox()` / `sandbox-pdf` detects and neutralizes JS actions, `javascript:` URIs, and document script trees
- **FR19.3**: Digital signature creation and verification
- **FR19.4**: Certificate management — ✅ PEM certificate store, import/list CLI, `/Cert` embedding on sign, extraction from signed PDFs
- **FR19.5**: DRM and permission enforcement

## Implementation Roadmap

### Phase 1: Foundation (Current)
✅ Basic PDF generation and parsing
✅ Markdown to PDF conversion
✅ Table rendering with borders and text wrapping
✅ Code blocks with syntax highlighting
✅ Font styles (bold/italic) and text alignment

### Phase 2: Performance (Next 2 weeks)
- [ ] FR12.1-12.4: Streaming processing
- [ ] FR13.1-13.4: Parallel processing
- [ ] Benchmarking suite

### Phase 3: Smart Features (1 month)
- [ ] FR14.1-14.5: Content analysis
- [ ] FR18.1-18.5: Intelligent optimization
- [ ] Machine learning integration prep

### Phase 4: Developer Experience (2 weeks)
- [ ] FR15.1-15.5: DX features
- [ ] FR15.3: Diff/patch support

### Phase 5: Web & Modern (1 month)
- [x] FR16.1-16.4: WASM support and canvas viewer
- [x] FR17.1-17.5: Advanced formats (PDF 2.0, PDF/A-3, PDF/UA, attachments, portfolios, U3D 3D annotations)
- [ ] FR16.5: Real-time collaborative editing

### Phase 6: Security & Advanced (1 month)
- [ ] FR19.1-19.5: Security features
- [ ] Production hardening
