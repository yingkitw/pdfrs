# pdfrs TODO List

This document tracks the planned features, improvements, and tasks for the **pdfrs** project.

## Priority Legend

- 🔴 **Critical**: Must-have for core functionality
- 🟡 **High**: Important features that significantly improve the tool
- 🟢 **Medium**: Nice-to-have features and enhancements
- 🔵 **Low**: Future considerations and minor improvements

---

## Phase 1: Core Functionality (Current Development)

### 🔴 Critical

- [x] Basic PDF generation from text
- [x] PDF parsing and text extraction
- [x] Markdown to PDF conversion
- [x] PDF to Markdown conversion
- [x] CLI interface with subcommands
- [x] Font selection (basic Type 1 fonts)
- [x] Multi-page support
- [x] Compression handling (deflate)
- [x] Table rendering from Markdown

### 🟡 High

- [x] Better text extraction with PDF operator handling
- [x] Image support framework
- [x] Error handling improvements
- [x] Performance optimizations
- [x] Roundtrip MD->PDF->MD with complex examples
- [x] PDF stream parsing for Tj text operators
- [x] Escaped parentheses handling in PDF strings
- [x] Integration tests for roundtrip validation (17 test cases)
- [x] Complex PDF generation examples validated via round-trip:
  - [x] `full_features.md` — broad element coverage (headings, lists, tables, code, images, …)
  - [x] `technical_report_complex.md` — dense tables, multi-language code, nested lists (23KB, 6+ pages)
  - [x] `api_reference_complex.md` — definitions, footnotes, code examples, feature matrix (28KB, 8+ pages)
  - [x] `math_and_formulas.md` — LaTeX math blocks/inline, code blocks, tables, formulas (27KB, 14 pages)
- [x] Library API integration tests (generate_pdf_bytes + validate_pdf_bytes, portrait + landscape batch)
- [x] Math parsing library API test (MathBlock + MathInline element detection + PDF generation)
- [x] Complex math formula PDF roundtrip test (limits, fraction, roots, integral/sum/product, set operators, quantifiers)
- [x] Extended math symbol coverage (mathbb sets, set relations, logic symbols, common function aliases) with regression tests
- [x] Rebalanced Unicode CID font default width to reduce text overlap while preserving compact spacing
- [x] Emit per-glyph CID `/W` widths + `/ToUnicode` CMap; fix rich inline styling/wrapping for comprehensive PDF quality
- [x] Text extraction prefers document ToUnicode maps (works with subset Identity-H fonts)
- [x] Display-math layout: stacked fractions, ∑/∏ limits above/below, ∫ side scripts
- [x] Code blocks use Unicode Type0 when embedded (CJK-safe) + wrap long lines to content width
- [x] Comprehensive fixture covers Chinese / Japanese / Korean prose and CJK-in-code samples
- [x] Multi-column layout (`PageLayout::with_columns`, `<!-- columns:N -->`, gutter rules, CLI `--columns`)
- [x] Markdown image embedding in md→PDF (`![alt](path)` → `/XObject`, relative to markdown/fixture dir)
- [x] Markdown charts (` ```chart bar|line|pie` ` → vector charts via `chart` module)
- [x] Academic thesis elements — `<!-- pagenumber:roman|arabic|none -->`, running headers, `<!-- toc -->`, figure/table numbering, `[@key]` citations + `<!-- bibliography -->`

---

## Phase 2: Enhanced Features

### 🔴 Critical

- [x] Complete image support implementation
  - [x] JPEG embedding with proper positioning (DCTDecode)
  - [x] PNG dimension parsing
  - [x] BMP dimension parsing
  - [x] Image scaling and optimization (aspect-ratio preserving)
  - [x] CLI add-image command wired up
  - [x] PNG pixel data embedding
  - [x] BMP pixel data embedding

### 🟡 High

- [x] Advanced PDF parsing
  - [x] Font encoding handling (WinAnsiEncoding, MacRomanEncoding)
  - [x] Text positioning and layout analysis (Td/Tm operator tracking)
  - [x] TJ array operator support for text extraction
  - [x] Improved dictionary parsing
  - [x] Octal escape handling in PDF strings
  - [x] Cross-reference stream parsing (for PDF 1.5+) — `parse_xref_stream` with /W field widths
  - [x] Object stream handling — `parse_object_stream` for /Type /ObjStm

- [x] Enhanced Markdown features
  - [x] Task list support
  - [x] Footnotes and references (definitions + inline ref stripping)
  - [x] Definition lists
  - [x] Strikethrough text
  - [x] Blockquote support (nested)
  - [x] Tables with alignment parsing (left/center/right)

- [x] PDF generation improvements
  - [x] Modularized `pdf_generator.rs` into focused helper submodules (`code_highlight`, `text_support`, `unicode_support`)
  - [x] Fixed mixed ASCII+Unicode rendering in Type0 font mode (prevent garbled ASCII headings when Unicode support is active)
  - [x] Text justification and alignment (H1 centered, TextAlign enum)
  - [x] Page numbering
  - [x] Header font size hierarchy (H1-H6)
  - [x] Code block reduced font size with background, border, and page-break support
  - [x] Horizontal rule rendering
  - [x] Watermarks — `watermark` CLI command (diagonal text, configurable opacity/size)
  - [x] Page orientation (landscape/portrait) with --landscape CLI flag
  - [x] Math/formula rendering (MathBlock with blue background + accent border, MathInline italic)
  - [x] LaTeX-to-text math conversion (Greek letters, operators, fractions, integrals, sums, limits)
  - [x] Inline math parsing inside rich paragraphs (`$...$` in standard paragraph lines)
  - [x] Unicode-aware text width estimation and wrapping (CJK/emoji-aware)
  - [x] UTF-16BE-safe text emission used consistently in line/code/math rendering paths
  - [x] Base-14 font compatibility fallback for unicode/math visibility (transliteration + `[U+XXXX]` marker)
  - [x] Math symbols render correctly in italic/oblique math paths under embedded Unicode font mode
  - [x] Regression tests for inline math detection in formatting parser
  - [x] Fixed font object ID references in PDF assembly
  - [x] Fixed table rendering crash with ragged row column counts

### 🟢 Medium

- [x] Font improvements
  - [x] Embedded font support
  - [x] TrueType font handling
  - [x] Unicode Type0/CIDFont resource generation with embedded FontFile2
  - [x] Glyph-ID CID text encoding for embedded Unicode font (fix garbled CJK/Greek/math rendering)
  - [x] Configurable unicode font path via `PDFRS_UNICODE_FONT_PATH`
  - [x] Font size variations within document (headers, code blocks)
  - [x] Text color support — `Color` struct (RGB), code blocks in gray

- [x] Security features
  - [x] Password protection — `PdfSecurity` with user/owner passwords
  - [x] User/owner permissions — `PdfPermissions` with PDF 1.7 compliance
  - [x] Digital signatures — `DigitalSignature` with `sign`/`verify-signature` CLI commands, SHA-256 content digest, PDF signature dictionary structure
  - [x] PDF sanitization — `PdfDocument::sanitize()` strips JavaScript, launch actions, external file references, additional actions; `sanitize-pdf` CLI command

- [x] Performance improvements
  - [x] Memory usage optimization — `parse_objects()` streams lines without allocating a full line index; lazy document retains single byte buffer
  - [x] Faster PDF parsing — compiled regex cache via `OnceLock` on hot parse/extract/validate paths
  - [x] Streaming processing for large files — `StreamingPdfGenerator`, `LazyPdfDocument`
  - [x] Parallel processing where applicable — `parallel.rs` with rayon

---

## Phase 3.5: Advanced Features (Surpassing Ghostscript)

### 🔴 Critical (Competitive Advantages)

#### FR12: Streaming & Incremental Processing
- [x] **FR12.1**: Streaming PDF generation — `StreamingPdfGenerator` struct with incremental page writing to `BufWriter<File>`
- [x] **FR12.2**: Page-by-page lazy loading — `render_page_range()` renders all elements, slices the resulting page streams to the requested `Range<usize>`, and assembles a standalone PDF with only the selected pages
- [x] **FR12.3**: Incremental PDF writing — `StreamingPdfGenerator::finish()` writes header, objects, xref, and trailer incrementally
- [x] **FR12.4**: Lazy PDF document — `LazyPdfDocument::load_from_bytes()` indexes stream object byte ranges without parsing dictionaries/arrays; `get_text()` lazily decompresses only content streams with text operators

#### FR13: Performance & Parallelism
- [x] **FR13.1**: `rayon` dependency for parallelism — `Cargo.toml` dependency and `parallel.rs` module
- [x] **FR13.2**: Parallel page rendering — `ParallelPdfGenerator::generate_markdown_pdfs_parallel()` with `par_iter()`
- [x] **FR13.3**: Parallel PDF merging — `merge_pdfs_parallel()` loads inputs concurrently, `extract_text_parallel()`, `validate_pdfs_parallel()`, `count_pages_parallel()`, `process_pdfs_parallel()`
- [x] **FR13.4**: SIMD text width calculations — `estimated_text_width()` now processes ASCII text in 8-byte chunks with unrolled `< 128` checks (auto-vectorizable by LLVM), falling back to scalar ASCII runs and per-char Unicode handling
- [x] **FR13.5**: Async PDF API for web servers — `async_api` module gated behind `async` Cargo feature; `tokio::fs` for non-blocking I/O, `tokio::task::spawn_blocking` for CPU-bound parsing/generation; `load_pdf_async()`, `generate_pdf_async()`, `optimize_pdf_async()`, `validate_pdf_async()`, `validate_pdf_a_async()`

#### FR15: Developer Experience
- [x] **FR15.1**: Builder API with fluent interface — `PdfBuilder` with `.with_layout()`, `.with_font()`, `.add_heading()`, `.add_paragraph()`, `.build()`, `.build_bytes()`
- [x] **FR15.2**: Property-based testing with `proptest` — `proptest` dev-dependency with tests in `compression.rs`, `image.rs`, `pdf_ops.rs`, `elements.rs`
- [x] **FR15.3**: Diff/patch support for version control — `diff_pdf_bytes()` compares two PDFs structurally (object count, page count, added/removed/modified objects, text similarity via Jaccard); `diff-pdfs` CLI command
- [x] **FR15.4**: Hot-reload during development — `watch-markdown` CLI command polls source file modification time and regenerates PDF on changes; configurable poll interval; `watch_markdown_to_pdf()` API function
- [x] **FR15.5**: Interactive REPL for PDF manipulation — `repl` CLI command with `load`, `save`, `text`, `pages`, `validate`, `validate-pdfa`, `optimize`, `sanitize`, `attach`, `info`, `help`, `quit` commands; stateful session with `PdfDocument`

#### FR18: Intelligent Optimization
- [x] **FR18.1**: Smart content-aware compression — `optimize-pdf` CLI command with stream recompression via `PdfDocument::to_bytes()`
- [x] **FR18.2**: Font subsetting to reduce file size — `prepare_unicode_font_support_with_subsetting()` with `subsetter` crate; collects used chars/glyphs, builds `GlyphRemapper`, subsets embedded TrueType font; `OptimizedPdfGenerator` respects `subset_fonts` setting; integration test verifies smaller PDF with embedded font
- [x] **FR18.3**: Object deduplication across pages — `PdfDocument::deduplicate_objects()` with deterministic content hashing and reference rewriting, integrated into `optimize_pdf_bytes()`
- [x] **FR18.4**: Optimization profiles (web, print, archive, ebook)
- [x] **FR18.6**: Linearized PDF (Fast Web View) — `linearize` module writes `/Linearized` dict with `/L` `/O` `/E` `/N` `/T`, first-page object priority; `linearize-pdf` CLI; Web/Ebook profiles set `linearize: true` and `optimize_pdf_bytes` / `OptimizedPdfGenerator` apply it
- [x] **FR18.7**: Incremental PDF updates — `incremental` module appends objects + xref/trailer with `/Prev`; `incremental_set_info` / `incremental_add_text_annotation`; `incremental-update` CLI; capability showcase validates prefix preservation
- [x] **Capability showcase** — `tests/fixtures/capability_showcase.md` + `tests/capability_validation.rs` (plugins, outlines, pages, linearize, incremental, vector/SVG/3D)
- [x] **Comprehensive PDF generator** — `comprehensive` module + bundled fixture `tests/fixtures/comprehensive_document.md`; CLI `generate-comprehensive` (default `comprehensive.pdf`); test `tests/comprehensive_pdf.rs` writes `tests/output/comprehensive_document.pdf` and root `comprehensive.pdf`

### 🟡 High Impact

#### FR14: Smart Content Analysis
- [x] **FR14.1**: Structure detection (headings, sections) — `detect-structure` CLI command with font-size heuristics
- [x] **FR14.2**: Table extraction to CSV/Excel formats — `extract-tables` CLI command
- [x] **FR14.3**: Form field detection and filling — `detect-form-fields` / `fill-form-fields` CLI commands
- [x] **FR14.4**: Content-aware image compression — `optimize_pdf_bytes` skips DCTDecode/JPXDecode/JBIG2Decode image streams to avoid re-wrapping already-compressed images in FlateDecode
- [x] **FR14.5**: PDF/A-1b validation — `validate-pdfa` CLI command with encryption, JS, font embedding, and XMP checks

#### FR16: WebAssembly Support
- [x] **FR16.1**: Add `wasm-bindgen` — optional `wasm-bindgen` dependency behind `wasm` Cargo feature; `rayon` made optional behind `parallel` feature so `wasm32-unknown-unknown` can compile without thread-dependent crates
- [x] **FR16.2**: WASM-compatible API — `wasm.rs` module with `render_markdown_to_pdf(md: &str) -> Result<Vec<u8>, JsValue>`; pure in-memory pipeline (no filesystem); `parallel` module conditionally compiled with `#[cfg(feature = "parallel")]`
- [x] **FR16.3**: JavaScript bindings and npm package — `wasm/package.json` with metadata and build script; `wasm/example.html` browser demo; `scripts/build-wasm.sh` for `wasm-pack` builds
- [x] **FR16.4**: Canvas-based PDF viewer in browser — `wasm/viewer.js` renders generated PDF bytes onto canvas via pdf.js; `wasm/example.html` demo with live preview and download

### 🟢 Medium

#### FR17: Advanced Format Support
- [x] **FR17.1**: PDF 2.0 specification features — `PdfVersion` enum (`V1_4`, `V2_0`) with `header()` and `supports_utf8_strings()`; added to `PageLayout` via `with_version()`; `PdfGenerator` uses version-aware header (`%PDF-2.0`); integration test verifies both versions generate valid, text-extractable PDFs
- [x] **FR17.2**: PDF/A-3 and PDF/UA validation — `validate_pdf_a3_bytes()` extends PDF/A-1b checks with embedded file requirement; `validate_pdf_ua_bytes()` checks /MarkInfo, /StructTreeRoot, /Lang, Title, no encryption, embedded fonts; `validate-pdfa3` and `validate-pdfua` CLI commands
- [x] **FR17.3**: Embedded file attachments — `PdfDocument::embed_file()` creates /EmbeddedFile stream and /Filespec objects, wires them into catalog's /Names -> /EmbeddedFiles name tree; `attach-file` CLI command
- [x] **FR17.4**: PDF portfolios and collections — `create_portfolio_pdf()` bundles multiple files into a portfolio PDF with `/Collection` catalog entry, schema (Name/Description columns), sort order, and embedded files; `create-portfolio` CLI command
- [x] **FR17.5**: 3D annotations (U3D) — `ThreeDAnnotation`, `create_pdf_with_3d_annotation` / `_bytes` embed `/Type /3D` `/Subtype /U3D` stream + `/Subtype /3D` annotation with `/3DD` / `/3DA`; `embed-3d` CLI; `pdf_contains_3d_u3d()` helper

#### FR19: Security
- [x] **FR19.1**: Malformed PDF sanitization — `PdfDocument::sanitize()` removes JavaScript (`/JS`, `/JavaScript`), launch actions (`/S /Launch`), external file references, additional actions (`/AA`), and `OpenAction` from catalog; `sanitize-pdf` CLI command
- [x] **FR19.2**: JavaScript action sandbox — `detect_javascript_actions()`, `PdfDocument::sandbox()`, `sandbox_pdf_bytes()` API; strips annotation `/A`/`/AA` JavaScript actions, `javascript:` URIs, and document-level script name trees; `sandbox-pdf` CLI command with action report
- [x] **FR19.3**: Digital signature creation/verification — `DigitalSignature` struct with SHA-256 content digest, `sign-pdf` and `verify-signature` CLI commands
- [x] **FR19.4**: Certificate management — `SigningCertificate`, `CertificateStore` (PEM import/list/get/remove), SHA-256 fingerprints, `/Cert` embedding in signatures, `extract_certificates_from_pdf()`; `import-certificate`, `list-certificates` CLI; `sign --certificate` / `--cert-id`

---

## Quick Wins (This Session)

### High Impact, Low Complexity
1. ✅ **Table border rendering** (COMPLETED)
2. ✅ **Code block text visibility** (COMPLETED)
3. ✅ **Text wrapping** (COMPLETED)
4. ✅ **FR12.3**: Streaming PDF write (wired `create-streaming` CLI command, fixed `StreamingPdfGenerator::finish` object ID generation)
5. ✅ **FR13.3**: Parallel PDF merge (wired to `merge` CLI command via `parallel::merge_pdfs_parallel`)
6. ✅ **FR15.1**: Builder API (fluent `PdfBuilder` in `src/builder.rs`, exported via `lib.rs`)
7. ✅ **FR18.4**: Optimization profiles (wired `--profile` to `create`/`md-to-pdf` CLI commands with real FlateDecode stream compression via `flate2`)

---

## Phase 3: Advanced Features

### 🟡 High

- [x] PDF manipulation features
  - [x] PDF merging (combine multiple PDFs) — `merge` CLI command
  - [x] PDF splitting (extract pages) — `split` CLI command
  - [x] Page reordering — `reorder` CLI command (comma-separated page order)
  - [x] Page rotation — `rotate` CLI command (0/90/180/270°)

- [x] Advanced image features
  - [x] Image filters and effects — `ImageFilter` (grayscale, invert, brightness, contrast, sepia), PNG scanline reconstruction, `apply_image_filters()` / `create_filtered_image_pdf()`; `filter-image` CLI
  - [x] Multiple images per page — `create_pdf_with_images` API
  - [x] Image overlay and watermarking
  - [x] Image extraction from PDFs — `extract-images` CLI command (JPEG DCTDecode + raw binary fallback)
  - [x] Vector graphics support — `vector` module with `VectorCanvas` / `VectorShape` (line, rect, ellipse, polygon, Bézier path); PDF operators `m`/`l`/`c`/`re`/`S`/`f`/`B`; `draw-vector` CLI demo

- [x] Form and annotation support
  - [x] Interactive form fields
  - [x] Text annotations — `TextAnnotation` + `create_pdf_with_annotations` API
  - [x] Link annotations — `LinkAnnotation` with URI actions
  - [x] Highlighting and markup — `HighlightAnnotation` with QuadPoints

- [x] Table extraction to CSV — `extract-tables` CLI command with position-based heuristic detection

### 🟢 Medium

- [x] Metadata handling
  - [x] Document properties (title, author, subject, keywords) — `md-to-pdf-meta` CLI
  - [x] Producer tag (pdf-cli)
  - [x] Custom metadata fields
  - [x] Metadata preservation during conversion

- [x] Accessibility features
  - [x] Tagged PDF structure types (`StructureType` enum, 35 types)
  - [x] `StructureElement` tree with alt_text, actual_text
  - [x] `element_to_structure()` mapping for all Element variants
  - [x] `AccessibilityOptions` builder (tagged_pdf, language, title)
  - [x] Full tagged PDF generation in output — `generate_tagged_pdf_bytes()` creates `/StructTreeRoot`, `/MarkInfo << /Marked true >>`, `/Lang`, and `/Title` in catalog/Info dictionary; generated PDFs pass `validate_pdf_ua_bytes()` checks
  - [x] Screen reader compliance testing — `check_screen_reader_compliance_bytes()` combines PDF/UA validation with text-extraction checks; `check-screen-reader` CLI command; integration test for complex tagged Markdown

- [x] Localization
  - [x] Multi-language error messages — `i18n` module (`Locale`, `MsgId`, `t`/`tf`); catalogs for en/es/de/fr/zh/he/ar; `localize_validation()`; CLI global `--lang` (also `PDFRS_LANG` / `LANG`)
  - [x] Locale-specific formatting — `format_integer` / `format_decimal` (thousands + decimal separators per locale)
  - [x] RTL text support — `rtl` module (Hebrew/Arabic detection, visual reorder, punctuation mirroring); auto RTL for RTL-dominant lines; `PageLayout::with_rtl()` / `md-to-pdf --rtl`

---

## Phase 4: Ecosystem and Integration

### 🟡 High

- [x] Library API
  - [x] Crate for use as a library (`pdf-rs` with `pub mod` exports)
  - [x] `generate_pdf_bytes()` — in-memory PDF generation without filesystem
  - [x] `validate_pdf()` / `validate_pdf_bytes()` — structural PDF validation
  - [x] `PdfValidation` result struct (errors, warnings, page_count, object_count)
  - [x] Rich `Element` enum with 27 variants for document modeling (math, charts, columns, thesis)
  - [x] `PdfDocument::load_from_bytes()` — in-memory PDF parsing without filesystem
  - [x] `PdfDocument::to_bytes()` — round-trip serialization for PDF optimization
  - [x] Rust API documentation (rustdoc with examples) — module-level `//!` docs added to all public modules (`elements`, `pdf_generator`, `pdf_ops`, `pdf`, `image`, `compression`, `builder`, `streaming`, `parallel`, `wasm`); cross-referenced types and runnable examples in `lib.rs`
  - [x] Example usage patterns (examples/ directory) — `examples/basic.rs` (generate PDF from Markdown), `examples/merge.rs` (merge PDFs), `examples/optimize.rs` (optimize with Web profile), `examples/watermark.rs` (add text watermark)

- [x] Plugin system
  - [x] Plugin architecture — `plugin` module with `ParserPlugin` / `GeneratorPlugin` traits and `PluginRegistry`
  - [x] Custom parser plugins — line hook via `parse_markdown_with_hook`; built-in `CalloutPlugin` (`:::note` / `:::warning` / …)
  - [x] Custom generator plugins — `transform_element` pass before PDF render; `PdfBuilder::add_markdown_with_plugins`
  - [x] Third-party integrations — CLI `md-to-pdf --plugins callouts`; register custom plugins via `PluginRegistry`
- [x] Document outlines / bookmarks — headings emit `/Outlines` + `/PageMode /UseOutlines` during PDF assembly

### 🟢 Medium

- [x] WebAssembly support
  - [x] Compile to WASM — `wasm-bindgen` integration with `wasm` Cargo feature; `wasm-pack` build via `scripts/build-wasm.sh`
  - [x] Browser-based PDF processing — `wasm/example.html` demo generates PDFs client-side with `render_markdown_to_pdf()`
  - [ ] Web interface

- [ ] Cloud integration
  - [ ] Cloud storage providers
  - [ ] Batch processing
  - [ ] REST API wrapper

---

## Quality and Maintenance Tasks

### 🔴 Critical

- [x] Comprehensive test suite (272 tests: 126 lib + 112 bin + 22 integration + 12 doc-tests)
  - [x] Unit tests for all modules (pdf, pdf_generator, pdf_ops, elements, markdown, image, compression)
  - [x] Integration tests for workflows (roundtrip, merge, split, rotate, watermark, reorder, metadata)
  - [x] Round-trip validation tests (generate → validate → parse → verify all element types)
  - [x] Performance benchmarks (criterion-based)
  - [x] Property-based tests (proptest for compression, image, pdf_ops, elements modules)
  - [ ] Automated testing pipeline

- [x] Documentation
  - [x] README.md with all CLI commands and examples
  - [x] ARCHITECTURE.md with module descriptions
  - [x] SPEC.md with functional requirements
  - [ ] API documentation (rustdoc with examples)
  - [ ] User guide
  - [x] Contributing guidelines — `docs/CONTRIBUTING.md`

### 🟡 High

- [ ] Code quality improvements
  - [ ] Code refactoring for maintainability
  - [ ] Error handling consistency
  - [ ] Memory safety verification
  - [ ] Security audit

- [ ] CI/CD improvements
  - [ ] Automated testing on multiple platforms
  - [ ] Automated release process
  - [ ] Performance regression testing
  - [ ] Dependency vulnerability scanning

### 🟢 Medium

- [ ] Monitoring and analytics
  - [ ] Usage statistics
  - [ ] Performance metrics
  - [ ] Error tracking
  - [ ] User feedback collection

---

## Research and Investigation

### 🔵 Low

- [x] PDF 2.0 specification research — `PdfVersion` enum with V1_4/V2_0; header generation; UTF-8 string support foundation laid
- [ ] Advanced compression algorithms
- [ ] Machine learning for OCR integration
- [x] Vector graphics (SVG) support — SVG path `d` parser (`M/L/H/V/C/S/Q/T/Z`) → `VectorCanvas`; `draw-svg` CLI (`--path` / `--file`)
- [x] 3D PDF support investigation — U3D 3D annotations via `embed-3d` (FR17.5)

### Audit follow-ups (2026-07-22)

- [x] Gate stubbed security encrypt/decrypt (`src/security.rs`) — protected paths return `Err`
- [x] Make `build_page_streams` return `Result` (in-memory generate + load; no silent empty Vec)
- [x] Align `streaming` with main generator (text fallbacks; layout directives no-op)
- [x] Propagate image embed / XObject failures instead of silent placeholder
- [x] Sync Element count (27) + SPEC multi-column FR + fix AGENTS.md domain copy
- [x] Tighten `comprehensive_pdf` asserts (Chart/Columns/Image/Toc); wire `syntect` highlighting
- [x] Split mega-files (incremental): accessibility → `pdf_generator/accessibility.rs`; REPL → `cli_repl.rs`; remove dead `generate_with_info`; dedupe `escape_pdf_string`; drop scratch examples
- [ ] Further splits: `ContentStreamBuilder`, `pdf_ops` domain clusters, `pdf` validation module

### Brainstorming (Competitive Intelligence — 2026-07)

Capabilities in peer projects worth prioritizing:

- **Native page rasterization** (Gigapdf, PDFium/PDFNova) — render PDF pages to PNG/canvas without pdf.js dependency
- **Full-text search with highlight boxes** (Gigapdf, PDF Oxide) — search within PDF and return bounding boxes
- **Office/HTML round-trip conversion** (Gigapdf) — DOCX/ODT/HTML ↔ PDF beyond Markdown
- **True redaction** (Gigapdf) — remove content from streams, not just opaque overlays
- **OCR for scanned PDFs** (Gigapdf) — built-in recognizer without Tesseract
- ~~**Vector path / SVG drawing**~~ — shipped: `vector` + SVG path `d` import (`draw-svg`); still open: full SVG docs (groups/transforms/text)
- ~~**Document outlines / bookmarks**~~ — shipped: `/Outlines` from headings + `/PageMode /UseOutlines`
- ~~**Linearized (web-optimized) PDF**~~ — shipped: `linearize` module + `linearize-pdf` CLI; Web/Ebook optimize profiles apply `/Linearized`
- ~~**Incremental PDF saves**~~ — shipped: `incremental` module (`incremental_set_info`, `incremental_add_text_annotation`, `/Prev` trailer); `incremental-update` CLI
- **PRC 3D / rich media** (Acrobat) — beyond U3D: PRC streams, `/RichMedia` annotations
- **Web Worker offloading** (MantisPDF) — keep UI responsive during large WASM operations
- **IndexedDB WASM module caching** (PDFNova) — instant reload of WASM binary in browser
- **Multi-language bindings** (PDF Oxide) — Python/JS/Go/C# from same Rust core
- ~~**Plugin hooks for Element → PDF**~~ — shipped: `ParserPlugin` / `GeneratorPlugin` + `CalloutPlugin`

---

## Long-term Vision

### Future Considerations

- [x] Full PDF 2.0 compliance — version header and `PdfVersion` support implemented; UTF-8 string encoding foundation in place
- [ ] GUI application
- [ ] Mobile app development
- [ ] Enterprise features
- [ ] Educational content and tutorials

---

## Timeline Estimates

### Phase 1 (Q1 2026): Core Foundation

- Core PDF functionality
- Basic CLI interface
- Initial testing

### Phase 2 (Q2 2026): Feature Enhancement

- Advanced parsing and generation
- Image support
- Performance improvements

### Phase 3 (Q3-Q4 2026): Advanced Features

- PDF manipulation
- Security features
- Form and annotation support

### Phase 4 (Q1 2027): Ecosystem

- Library API
- Plugin system
- WebAssembly support

---

## Resource Planning

### Team Structure (Future)

- **Core Developers**: PDF spec experts, Rust developers
- **QA Engineers**: Testing and quality assurance
- **Documentation Writers**: User guides and API docs
- **Community Managers**: User support and feedback

### Technology Stack

- **Core**: Rust (for performance and safety)
- **Testing**: Rust testing framework, property testing
- **CI/CD**: GitHub Actions or similar
- **Documentation**: Markdown, mdBook
- **Distribution**: Cargo, crates.io

---

## Risk Assessment

### Technical Risks

- **PDF Complexity**: The PDF specification is vast and complex
- **Performance**: Large file processing may be challenging
- **Compatibility**: Ensuring broad PDF format support

### Mitigation Strategies

- **Incremental Development**: Build features incrementally
- **Community Involvement**: Leverage community knowledge
- **Extensive Testing**: Comprehensive test coverage

---

## Success Metrics

### Technical Metrics

- **Performance**: <1s for 1MB PDF processing
- **Memory**: <100MB for typical operations
- **Compatibility**: Support for 90% of common PDFs

### User Metrics

- **Adoption**: Growing user base
- **Contributions**: Community involvement
- **Issues**: Low bug rate, quick resolution

---

This TODO list serves as a roadmap for the **pdfrs** project, guiding development priorities and ensuring a structured approach to feature implementation and quality improvement.
