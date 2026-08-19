# Changelog

All notable changes to **pdfrs** are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Security

- **Spec-conformant encryption** (`src/security.rs`): the Standard Security
  Handler was rewritten to the PDF 1.7 / ISO 32000-2 algorithms. V1-V4 now
  implement full Algorithm 2 (including the trailer `/ID` input),
  Algorithm 3.3 (`/O` with the 19-round RC4 chain), and Algorithms
  3.4/3.5 (`/U`). AES-256 now emits revision 6 (V5/R6): random 32-byte
  file key, random validation/key salts, `/U`/`/O`/`/UE`/`/OE` per
  Algorithms 2.A-2.F, and the hardened iterative Algorithm 2.B hash
  (AES-128-CBC zero-padded rounds with SHA-256/384/512 selection). AES-CBC
  stream encryption uses a fresh random IV per object instead of a
  deterministic hash. New `getrandom` dependency (non-wasm targets;
  encryption returns a clear error on `wasm32-unknown-unknown`).
  `PdfSecurity::generate_encryption_materials(&id)` replaces the broken
  dict/key entry points, which produced deterministic keys and
  non-standard `/O`/`/U` values that no conforming reader would accept.
- **Byte-safe document encryption** (`src/pdf_ops/security.rs`):
  `encrypt_pdf_bytes` no longer round-trips binary PDFs through
  `String::from_utf8_lossy` (which corrupted Flate streams and could panic
  on offset splicing). Objects are scanned byte-precisely with
  `/Length`-verified stream boundaries, streams and literal strings
  (escape-decoded, nesting-aware) are encrypted, `/Length` entries are
  rewritten, and the output is rebuilt with a fresh xref table, a
  trailer carrying `/Encrypt`, `/ID`, `/Root`, `/Info`, and a proper
  `startxref`/`%%EOF` chain. Documents using object streams or
  cross-reference streams are rejected with a clear error instead of
  being corrupted.
- **True image redaction** (`src/redact.rs`): redacting an image region now
  removes the image XObject object itself from the document (after
  verifying nothing else references it), not just the `Do` operator — the
  image bytes can no longer be extracted from the output.
- **Redaction coverage**: text-showing operators are now masked even
  outside `BT…ET`, and Form XObjects plus annotation appearance streams
  (`/AP`) referenced from the page are rewritten too.


### Added

- **CI restored** (`.github/`): fmt + strict clippy (default and `api`
  features), multi-OS test matrix, `api` feature job, WASM build,
  minimal-features build/test, benchmark compile check, `cargo-audit`
  (push/PR + weekly), and tag-triggered publish workflow.
- `encrypt_pdf_bytes_with_id` — encryption variant returning the file key
  and accepting an explicit `/ID` (for callers that need to decrypt later).
- `sign_pdf_bytes` — in-memory digital-signature incremental update.
- 17 new tests: encryption roundtrips for all four algorithms
  (byte-exact stream decryption, structure, xref validation, AES-256
  non-determinism), object-stream rejection, sign offset checks, redaction
  of images from documents, outside-`BT` masking, HTML depth-cap safety,
  API body-limit 413, R6 hash self-consistency, and RC4 empty-key error.
  Test count: 473 → 487.
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
- **PDF encryption** (`src/security.rs`): RC4 40-bit, RC4 128-bit,
  AES-128-CBC, and AES-256-CBC encryption and decryption (later reworked
  to fully spec-conformant algorithms — see Security above). New crates:
  `md-5`, `aes`, `cbc`.
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
  (`/api/v1/extract`), and health check (`/api/v1/health`). CORS is
  configurable via `router_with_cors` (no permissive default).
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


### Changed

- **API hardening** (`src/api.rs`): request bodies are limited via
  `RequestBodyLimitLayer` (50 MB default, `AppState.max_body`), CPU-heavy
  handlers run in `tokio::task::spawn_blocking`, CORS is no longer
  permissive by default (`router_with_cors` attaches an explicit policy),
  and `api::serve` returns `Result` instead of `unwrap`/`process::exit`.
- **Regex caching**: all 26 hot-path `Regex::new` sites across
  `pdf_ops/{forms,structure,tables,security}`, `incremental`, `vector`,
  and `cli_repl` now use the `OnceLock` pattern via local `*_regex!`
  macros (matching `pdf.rs`/`elements.rs` conventions).
- Rasterizer allocations are clamped to 32,768 px per dimension and
  CIDFont `/W` ranges are bounded (65,535 glyphs max) to guard against
  malformed documents.
- HTML conversion caps DOM recursion at 128 levels; deeper subtrees are
  flattened iteratively so deep content still converts without stack risk.
- **Module split**: extracted the PDF validation cluster (structural, PDF/A-1b,
  PDF/A-3b, PDF/UA-1, screen reader compliance) from `src/pdf.rs` into a new
  `src/pdf/validation.rs` submodule. Public API is unchanged — all items are
  re-exported at `crate::pdf::`, so `crate::pdf::validate_pdf_bytes` and
  friends continue to work without code changes. `src/pdf.rs` shrank by ~430
  lines. Added 6 focused unit tests inside the new module; total test count
  grew from 389 → 395.


### Fixed

- `rc4_encrypt` returns an error on empty keys instead of panicking
  (`key[i % key.len()]` division-by-zero equivalent).
- Image redaction placement now maps the unit square through the full CTM
  (bbox of all four corners) instead of multiplying `/Width`×`CTM[0]`.
- `sign_pdf` incremental updates: object numbers are allocated past the
  document maximum instead of the colliding `999`, the original catalog is
  copied and extended (not hardcoded `1 0 R`), the digest/`/ByteRange`
  splice is byte-level with fixed-width values (no more lossy-UTF-8
  rewrite of the entire file), and malformed `startxref` is an error
  instead of a slice panic.
- `escape_pdf_name` applied `#` escaping after injecting `#20`, mangling
  names containing spaces.
- Redaction no longer truncates output at the last `/` in the stream when
  removing a `Do` (name operands are tracked explicitly, so `/` inside
  string literals survives).
- `search::collect_font_metrics` only visited catalog-level resources;
  it now scans every object's `/Resources`, so per-page font width tables
  resolve for text extraction, search, and redaction.
- `search::decompress_stream` now validates the full zlib header
  (mod-31 FCHECK), matching `pdf.rs`, and both copies are unified.
- Removed dead code: unused `TextSpan`/`SpanCollector` fields in
  `pdf_to_md.rs`, test-only `prepare_unicode_font_support` gated behind
  `#[cfg(test)]`.

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
