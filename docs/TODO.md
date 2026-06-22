# PDF-CLI TODO List

This document tracks the planned features, improvements, and tasks for the PDF-CLI project.

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
  - [x] `full_features.md` — all 17 element types (10KB, 6 pages)
  - [x] `technical_report_complex.md` — dense tables, multi-language code, nested lists (23KB, 6+ pages)
  - [x] `api_reference_complex.md` — definitions, footnotes, code examples, feature matrix (28KB, 8+ pages)
  - [x] `math_and_formulas.md` — LaTeX math blocks/inline, code blocks, tables, formulas (27KB, 14 pages)
- [x] Library API integration tests (generate_pdf_bytes + validate_pdf_bytes, portrait + landscape batch)
- [x] Math parsing library API test (MathBlock + MathInline element detection + PDF generation)
- [x] Complex math formula PDF roundtrip test (limits, fraction, roots, integral/sum/product, set operators, quantifiers)
- [x] Extended math symbol coverage (mathbb sets, set relations, logic symbols, common function aliases) with regression tests
- [x] Rebalanced Unicode CID font default width to reduce text overlap while preserving compact spacing

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

- [ ] Performance improvements
  - [ ] Memory usage optimization
  - [ ] Faster PDF parsing
  - [ ] Streaming processing for large files
  - [ ] Parallel processing where applicable

---

## Phase 3.5: Advanced Features (Surpassing Ghostscript)

### 🔴 Critical (Competitive Advantages)

#### FR12: Streaming & Incremental Processing
- [ ] **FR12.1**: Streaming PDF generation trait
  ```rust
  pub trait StreamingPdfGenerator {
      fn generate_streaming(&mut self, elements: &[Element]) -> Stream<Page>;
  }
  ```
- [ ] **FR12.2**: Page-by-page lazy loading
  ```rust
  pub fn render_page_range(&mut self, elements: &[Element], range: Range<usize>) -> Result<Vec<Page>>;
  ```
- [ ] **FR12.3**: Incremental PDF writing (write pages as generated)
  ```rust
  pub fn create_pdf_streaming(filename: &str, elements: &[Element]) -> Result<()>;
  ```
- [ ] **FR12.4**: Lazy PDF document (load pages on-demand)
  ```rust
  pub struct LazyPdfDocument { /* ... */ }
  ```

#### FR13: Performance & Parallelism
- [ ] **FR13.1**: Add `rayon` dependency for parallelism
- [ ] **FR13.2**: Parallel page rendering with `par_iter()`
- [ ] **FR13.3**: Parallel PDF merging (load inputs concurrently)
- [ ] **FR13.4**: SIMD text width calculations
- [ ] **FR13.5**: Async PDF API for web servers (`tokio`)

#### FR15: Developer Experience
- [ ] **FR15.1**: Builder API with fluent interface
  ```rust
  PdfBuilder::new().with_layout(PageLayout::landscape()).build()?;
  ```
- [ ] **FR15.2**: Property-based testing with `proptest`
- [ ] **FR15.3**: Diff/patch support for version control
- [ ] **FR15.4**: Hot-reload during development
- [ ] **FR15.5**: Interactive REPL for PDF manipulation

#### FR18: Intelligent Optimization
- [x] **FR18.1**: Smart content-aware compression — `optimize-pdf` CLI command with stream recompression via `PdfDocument::to_bytes()`
- [ ] **FR18.2**: Font subsetting to reduce file size
- [ ] **FR18.3**: Object deduplication across pages
- [x] **FR18.4**: Optimization profiles (web, print, archive, ebook)

### 🟡 High Impact

#### FR14: Smart Content Analysis
- [x] **FR14.1**: Structure detection (headings, sections) — `detect-structure` CLI command with font-size heuristics
- [x] **FR14.2**: Table extraction to CSV/Excel formats — `extract-tables` CLI command
- [x] **FR14.3**: Form field detection and filling — `detect-form-fields` / `fill-form-fields` CLI commands
- [ ] **FR14.4**: Content-aware image compression
- [x] **FR14.5**: PDF/A-1b validation — `validate-pdfa` CLI command with encryption, JS, font embedding, and XMP checks

#### FR16: WebAssembly Support
- [ ] **FR16.1**: Add `wasm-bindgen` and `wasm-pack`
- [ ] **FR16.2**: WASM-compatible API
  ```rust
  #[wasm_bindgen]
  pub fn render_markdown_to_pdf(md: &str) -> Result<Vec<u8>, JsValue>;
  ```
- [ ] **FR16.3**: JavaScript bindings and npm package
- [ ] **FR16.4**: Canvas-based PDF viewer in browser

### 🟢 Medium

#### FR17: Advanced Format Support
- [ ] **FR17.1**: PDF 2.0 specification features
- [ ] **FR17.2**: PDF/A-3 and PDF/UA (accessibility)
- [ ] **FR17.3**: Embedded file attachments
- [ ] **FR17.4**: PDF portfolios and collections
- [ ] **FR17.5**: 3D annotations (U3D)

#### FR19: Security
- [ ] **FR19.1**: Malformed PDF sanitization
- [ ] **FR19.2**: JavaScript action sandbox
- [ ] **FR19.3**: Digital signature creation/verification
- [ ] **FR19.4**: Certificate management

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
  - [ ] Image filters and effects
  - [x] Multiple images per page — `create_pdf_with_images` API
  - [x] Image overlay and watermarking
  - [x] Image extraction from PDFs — `extract-images` CLI command (JPEG DCTDecode + raw binary fallback)
  - [ ] Vector graphics support

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
  - [ ] Full tagged PDF generation in output
  - [ ] Screen reader compliance testing

- [ ] Localization
  - [ ] Multi-language error messages
  - [ ] Locale-specific formatting
  - [ ] RTL text support

---

## Phase 4: Ecosystem and Integration

### 🟡 High

- [x] Library API
  - [x] Crate for use as a library (`pdf-rs` with `pub mod` exports)
  - [x] `generate_pdf_bytes()` — in-memory PDF generation without filesystem
  - [x] `validate_pdf()` / `validate_pdf_bytes()` — structural PDF validation
  - [x] `PdfValidation` result struct (errors, warnings, page_count, object_count)
  - [x] Rich `Element` enum with 19 variants for document modeling (including MathBlock, MathInline)
  - [x] `PdfDocument::load_from_bytes()` — in-memory PDF parsing without filesystem
  - [x] `PdfDocument::to_bytes()` — round-trip serialization for PDF optimization
  - [ ] Rust API documentation (rustdoc with examples)
  - [ ] Example usage patterns (examples/ directory)

- [ ] Plugin system
  - [ ] Plugin architecture
  - [ ] Custom parser plugins
  - [ ] Custom generator plugins
  - [ ] Third-party integrations

### 🟢 Medium

- [ ] WebAssembly support
  - [ ] Compile to WASM
  - [ ] Browser-based PDF processing
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
  - [ ] Contributing guidelines

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

- [ ] PDF 2.0 specification research
- [ ] Advanced compression algorithms
- [ ] Machine learning for OCR integration
- [ ] Vector graphics (SVG) support
- [ ] 3D PDF support investigation

---

## Long-term Vision

### Future Considerations

- [ ] Full PDF 2.0 compliance
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

This TODO list serves as a roadmap for the PDF-CLI project, guiding development priorities and ensuring a structured approach to feature implementation and quality improvement.
