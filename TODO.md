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
- [x] Library API integration tests (generate_pdf_bytes + validate_pdf_bytes, portrait + landscape batch)

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
  - [x] Text justification and alignment (H1 centered, TextAlign enum)
  - [x] Page numbering
  - [x] Header font size hierarchy (H1-H6)
  - [x] Code block reduced font size
  - [x] Horizontal rule rendering
  - [x] Watermarks — `watermark` CLI command (diagonal text, configurable opacity/size)
  - [x] Page orientation (landscape/portrait) with --landscape CLI flag

### 🟢 Medium

- [ ] Font improvements
  - [ ] Embedded font support
  - [ ] TrueType font handling
  - [x] Font size variations within document (headers, code blocks)
  - [x] Text color support — `Color` struct (RGB), code blocks in gray

- [x] Security features
  - [x] Password protection — `PdfSecurity` with user/owner passwords
  - [x] User/owner permissions — `PdfPermissions` with PDF 1.7 compliance
  - [ ] Digital signatures

- [ ] Performance improvements
  - [ ] Memory usage optimization
  - [ ] Faster PDF parsing
  - [ ] Streaming processing for large files
  - [ ] Parallel processing where applicable

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
  - [ ] Vector graphics support

- [x] Form and annotation support
  - [x] Interactive form fields
  - [x] Text annotations — `TextAnnotation` + `create_pdf_with_annotations` API
  - [x] Link annotations — `LinkAnnotation` with URI actions
  - [x] Highlighting and markup — `HighlightAnnotation` with QuadPoints

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
  - [x] Rich `Element` enum with 17 variants for document modeling
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

- [x] Comprehensive test suite (258 tests: 115 lib + 112 bin + 20 integration + 11 bench)
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
