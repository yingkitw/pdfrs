# Changelog

All notable changes to **pdfrs** are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

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
