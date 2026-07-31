# Changelog

All notable changes to **pdfrs** are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- **Rasterizer text rendering** (`src/raster.rs`): three bugs conspired to
  render all text as gray bars. (1) `extract_font_name` read the font *size*
  token instead of the *name* token before `Tf`, and the `Tf` handler wrongly
  required two numeric operands (the `/Name` token is not numeric), so font
  metrics were never resolved. (2) Glyph outlines were positioned with the
  text-matrix translation applied twice, throwing them off-page. (3) CIDFont
  `/W` arrays (which live on the descendant CIDFont for Type0 fonts) were
  never parsed, so every glyph used the default advance and overlapped.
  Base-14 fonts in the raw-scan path now use the built-in width tables.
- **Search on CID-keyed PDFs** (`src/search.rs`): `search-pdf` found no
  matches in documents using Type0/Identity-H fonts because hex string
  operands were decoded as UTF-8 instead of via the document's `/ToUnicode`
  CMap. Text extraction now uses `decode_pdf_hex_string_with_map`.
- **Table width** (`src/table_renderer.rs`): tables narrower than the content
  area no longer stay cramped; columns expand proportionally to fill the
  available page width.

### Added

- **Glyph-outline rasterization** (`src/raster.rs`): native PDF rasterizer now
  renders actual glyph outlines from embedded TrueType fonts (TTF) instead of
  schematic gray rectangles. Extracts `/FontFile2` streams from font
  descriptors, including Type0 fonts via `/DescendantFonts`. Uses `ttf-parser`
  for glyph outline parsing and fills flattened Bézier curves as polygons.
  Falls back to gray rectangles for base-14 fonts or fonts without embedded
  data. Raw PDF byte scanning works around the whitespace-tokenised dict
  parser's truncation of inline dictionaries.
- **Basic CSS support** (`src/html.rs`): HTML-to-PDF pipeline now parses
  `<style>` tags and inline `style` attributes. Supports `font-weight`,
  `font-style`, `text-align`, `color`, `background-color`, `font-size`,
  `margin`, `padding`, and `border` properties. Selectors: tag (`p`), class
  (`.classname`), tag.class (`p.highlight`), and id (`#id`). CSS rules
  cascade with inline styles taking highest priority.
- **Real PDF encryption** (`src/security.rs`): RC4 40-bit, RC4 128-bit,
  AES-128-CBC, and AES-256-CBC encryption and decryption per PDF 1.7
  Standard Security Handler. MD5/SHA-256 key derivation, PDF standard
  password padding, PKCS#7 padding for AES. `pdf_ops::security::protect_pdf`
  now encrypts streams and strings, inserts `/Encrypt` dictionary, and
  patches the trailer. New crates: `md-5`, `aes`, `cbc`.
- **Redaction improvements** (`src/redact.rs`): image XObject removal
  (detects `Do` operators referencing images whose CTM placement
  intersects redaction regions and removes them) and partial-string
  redaction (masks only individual characters whose bounding boxes
  intersect redaction regions, preserving surrounding text). CTM tracking
  for accurate image placement detection.
- **Multi-series stacked bar charts** (`src/chart.rs`,
  `src/elements.rs`, `src/pdf_generator/content_stream.rs`): new
  `ChartKind::StackedBar` variant and `ChartSeries` struct for named
  multi-series data. `series:` directive in chart fence declares series
  names; data lines use `Label, v1, v2, v3` format. Legend with colored
  series indicators rendered below chart.
- **REST API wrapper** (`src/api.rs`, behind `api` feature): axum-based
  HTTP server with endpoints for PDF generation (`/api/v1/generate`),
  merge (`/api/v1/merge`), split (`/api/v1/split`), search
  (`/api/v1/search`), redaction (`/api/v1/redact`), text extraction
  (`/api/v1/extract`), and health check (`/api/v1/health`). CORS enabled.
  New crates: `axum`, `tower-http`, `base64`. New byte-based helpers:
  `pdf_ops::merge_pdfs_from_bytes`, `pdf_ops::split_pdf_from_bytes`.
- **WASM polish**: Web Worker offloading (`wasm/worker.js`,
  `wasm/worker-client.js`) for off-main-thread PDF generation with
  zero-copy transfer. IndexedDB caching (`wasm/cache.js`) for the
  compiled WASM binary, keyed by crate version for auto-invalidation.
  New `version()` export for cache key management. `syntect` switched
  to `default-fancy` (pure Rust regex) for WASM compatibility.
  Updated `example.html` with mode toggle (worker vs main thread).
- **GitHub Actions CI** (`.github/workflows/ci.yml`): automated testing
  pipeline with rustfmt check, clippy (advisory), multi-OS test matrix
  (ubuntu/macos/windows), WASM build verification, minimal-feature build +
  test, and criterion benchmark compile check.
- **Security audit workflow** (`.github/workflows/audit.yml`): `cargo-audit`
  on every push/PR plus weekly schedule for dependency vulnerability scanning.
- **Release workflow** (`.github/workflows/release.yml`): tag-triggered
  `cargo publish` to crates.io + GitHub Release with auto-generated notes.
- `rust-toolchain.toml` pins stable Rust with rustfmt + clippy components.

### Fixed

- **WASM build**: `main.rs` unconditionally imported `parallel` module
  (feature-gated behind `parallel`). Split into conditional import with
  sequential `pdf_ops::merge_pdfs` fallback when `parallel` feature is off.
- **Formatting**: `cargo fmt` applied to fix `cargo fmt --check` failures.
- **Clippy**: resolved all 57 clippy warnings (41 auto-fixed, 16 manual).
  Key fixes: regex-in-loop → `OnceLock` cached regexes; collapsed nested
  matches; `vec![]` → array literal; `as_bytes` after slice; unused variable
  prefixed with `_`; `#[allow(clippy::type_complexity)]` and
  `#[allow(clippy::too_many_arguments)]` on public API functions where
  refactoring would harm readability. CI now enforces `clippy -- -D warnings`.
- **Security: path traversal in `CertificateStore`**: `import`, `get`, and
  `remove` methods used the `id` parameter directly in file paths without
  sanitization. Added `validate_cert_id()` that rejects empty IDs and IDs
  containing `/`, `\`, or `..`. Regression test covers all attack vectors.
- **Misleading function name**: `flatten_cubic_into_unsafe` in `raster.rs`
  contained no `unsafe` code; renamed to `flatten_cubic_into_segments`.

### Changed

- **Module split**: extracted the PDF validation cluster (structural, PDF/A-1b,
  PDF/A-3b, PDF/UA-1, screen reader compliance) from `src/pdf.rs` into a new
  `src/pdf/validation.rs` submodule. Public API is unchanged — all items are
  re-exported at `crate::pdf::`, so `crate::pdf::validate_pdf_bytes` and
  friends continue to work without code changes. `src/pdf.rs` shrank by ~430
  lines. Added 6 focused unit tests inside the new module; total test count
  grew from 389 → 395.

## [0.2.0] — 2026-07-26

Five new capabilities, all pure Rust with **no new dependencies**. Test count
grew from 336 → 389.

### Added

- **Native PDF → PNG rasterization** (`src/raster.rs`, ~1700 LOC).
  Pure-Rust rasterizer with an inline PNG encoder (signature + IHDR +
  zlib-compressed IDAT via `flate2` + IEND, built-in CRC-32). Renders the
  operators emitted by `pdfrs` plus the common content-stream subset from
  other producers (`q`/`Q`, `cm`, color ops, path construction, path
  painting, `BT`/`ET`, `Tf`, text-positioning ops, `Tj`/`TJ`). Base-14 PDF
  font width tables (Helvetica, Times-Roman, Courier). Text is rendered as
  gray glyph-block rectangles sized to advance widths (schematic rasterizer).
  New APIs: `raster::rasterize_page`, `raster::rasterize_all`,
  `RasterPage::to_png`. CLI: `rasterize-pdf`.

- **Full-text search with per-hit bounding boxes** (`src/search.rs`,
  ~1240 LOC). Walks each page's content stream, computes the bounding
  rectangle of every text-show operation, and matches the query
  (case-insensitive substring). Returns `Vec<SearchHit>` with page, matched
  text, snippet, and `Rect` bbox. `Rect::intersects` / `Rect::contains`
  helpers for viewer/redaction integration. CLI: `search-pdf` with optional
  `--json` output. Also the shared content-stream helpers hub used by the
  raster, redact, and pdf_to_md modules.

- **True content-stream redaction** (`src/redact.rs`, ~480 LOC). Rewrites
  page content streams to mask intersecting text instead of relying on
  opaque overlays. `RedactionStyle::BlackBox` (default) replaces intersecting
  text with whitespace-equivalent masks AND appends a solid-black filled
  rectangle over each region; `RedactionStyle::Strip` masks text without the
  overlay. Stream compression preserved (FlateDecode streams are recompressed
  after rewriting). CLI: `redact-pdf` with repeatable
  `--region page,x,y,w,h`.

- **Full SVG document rendering** (`src/vector.rs`, ~900 LOC added). Parses
  a full SVG document with an inline minimal XML parser: `<svg>`, `<g>`,
  `<rect>`, `<circle>`, `<ellipse>`, `<line>`, `<polyline>`, `<polygon>`,
  `<path d="...">`, `<text>`, `<defs>`, `<symbol>`, `<tspan>`. Transform
  composition via `parse_svg_transform` supporting `translate`, `scale`,
  `rotate` (with optional centre), `matrix`, `skewX`, `skewY`. Style
  attributes (`fill`, `stroke`, `stroke-width`, `opacity`) inherited through
  parent `<g>`. Paint parsing supports named colours, `#rgb`/`#rrggbb` hex,
  and `rgb(r,g,b)`. Y-axis flipped so SVG top-left maps to PDF bottom-left.
  New APIs: `parse_svg_document`, `svg_document_to_pdf_bytes`,
  `svg_document_file_to_pdf`. CLI: `draw-svg-file`. Backwards compatible
  with existing `extract_svg_path_d` / `svg_path_to_pdf_bytes`.

- **Structured PDF → Markdown conversion** (`src/pdf_to_md.rs`, ~520 LOC).
  Replaces the plain-text dump produced by `pdf::extract_text`. Walks
  content streams, groups text spans into lines by Y proximity, emits
  real Markdown. Body font size detected by character-count-weighted mode.
  Heading levels 1-5 inferred from `line.max_font_size / body_size` ratios.
  Bullet lists, numbered lists, code blocks (Courier detection), and
  horizontal rules reconstructed. ToUnicode-aware decoding of CID-font
  glyph-ID hex strings. CLI: `pdf-to-md` upgraded; falls back to plain
  `extract_text` on conversion errors.

- **Integration tests** (`tests/capabilities_v2.rs`, 7 end-to-end tests):
  rasterize→search→redact round trip, full SVG document, PDF→MD structure,
  multi-page rasterize, search page attribution, strip-style redact, SVG
  transform composition.

### Changed

- README, SPEC (FR20-FR24), TODO (brainstorming items checked off),
  ARCHITECTURE (5 new module sections) all updated.
- Shared content-stream helpers (`collect_pages_from_doc`,
  `collect_font_metrics`, `tokenize`, `extract_string`, `extract_tj_array`,
  `extract_font_name`, `decompress_stream`, `as_ref_id`, `parse_kids_string`,
  `raw_kids_for_object`) made `pub(crate)` in `search.rs` so the new modules
  reuse a single implementation.
- `pdf::collect_tounicode_gid_map` and `pdf::decode_pdf_hex_string_with_map`
  promoted to `pub(crate)` so `pdf_to_md` can decode CID-font glyph IDs.
- `collect_pages_from_doc` accepts an optional raw-bytes slice and falls back
  to `raw_kids_for_object` to recover from the whitespace-tokenised dict
  parser in `pdf.rs` (which truncates `/Kids [a b c]` to `[a`).

## [0.1.5] — 2026-07

Initial crates.io release: Markdown ↔ PDF, Unicode/CJK with embedded TTF,
charts, multi-column, thesis TOC/citations, merge/split/rotate/reorder,
watermark, annotations, forms, linearized + incremental PDF, PDF/A + PDF/UA
validation, tagged PDF generation, sanitization, sandboxing, digital
signatures + certificate store, plugin system, builder API, WASM build,
streaming + parallel generators, optimization profiles, and `pdfcli` binary.

[Unreleased]: https://github.com/yingkitw/pdfrs/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/yingkitw/pdfrs/releases/tag/v0.2.0
[0.1.5]: https://github.com/yingkitw/pdfrs/releases/tag/v0.1.5
