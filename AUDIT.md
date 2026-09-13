# pdfrs Codebase Audit Report

**Date:** 2026-09-13 (re-audit + remediation)
**Scope:** Full re-audit of `src/`, `tests/`, `examples/`, `Cargo.toml`, root docs, CI, and release artifacts.
**Methodology:** Tool-verified test/doc builds, source review, docs-vs-code cross-check, and release verification.
**Status:** Strong production baseline with a small set of explicit compatibility and performance follow-ups.

## Codebase Score (2026-09-13)

**Overall: 8.8/10**

| Dimension | Score | Evidence / deduction |
|---|---:|---|
| Correctness and reliability | 9.2/10 | 508 tests pass, including round-trip and property-based coverage; some PDF feature boundaries remain explicit. |
| Security | 9.0/10 | Encryption, redaction, sandboxing, path validation, and audit CI are covered; object/xref stream encryption now has integration coverage. |
| Maintainability | 8.5/10 | Large modules were decomposed and typed errors were added; the 37k-line Rust surface still warrants focused ownership and profiling. |
| Performance | 8.0/10 | Batch generation was reduced from ~69s to ~15s; the all-algorithm encryption integration test still takes about 47–60s. |
| Documentation and release hygiene | 8.6/10 | Docs, rustdoc, tests, and crates.io publication are verified; historical audit/TODO entries need continued cleanup. |
| CI and portability | 8.7/10 | CI covers formatting, clippy, tests, WASM, minimal features, and advisories; synchronous WASM exports remain a known limitation. |

### Priority findings

1. **Medium — object-stream encryption expands compressed objects** and replaces xref streams; test interoperability with external PDF readers.
2. **Medium — encryption integration coverage is slow** because all four encryption algorithms are exercised in one long test.
3. **Medium — WASM's synchronous public export can block the main thread**; keep the worker API as the recommended path.
4. **Low — historical audit and TODO counts/statuses can drift**; keep one current score and verification record at the top of this file.

---

## Remediation Update (2026-09-13)

| Finding | Status | Resolution |
|---------|--------|------------|
| M8 god modules | ✅ Fixed | `pdf.rs` → `pdf/` (objects, parser, text_extract, sandbox, diff, decode, validation); `raster.rs` → `raster/` (surface, interpreter, fonts, base14, png, pdf_access); `vector.rs` → `vector/` (path, svg_document, xml, transform, emit); `content_stream.rs` → `content_stream/` (builder, elements, charts, math, page_assembly, render). Public APIs re-exported unchanged. |
| M9 `main.rs` monolith | ✅ Fixed | Thin entry point + `src/cli/` (`args.rs`, dispatch `mod.rs`, `commands/{generation,conversion,manipulate,vector_raster,inspect,security,service}.rs`). `--help` verified byte-identical. |
| H6 anyhow-only error model | ✅ Fixed | `src/error.rs`: `PdfError` (Io/InvalidPdf/Parse/PageNotFound/Crypto/Image/Svg/InvalidInput/Unsupported/Context/Other) + `pdfrs::Result<T>`; all ~127 library `anyhow!` sites migrated; CLI/`serve()` keep `anyhow` through automatic `std::error::Error` conversion. Breaking API change, documented in CHANGELOG. |
| Rasterizer fidelity gap | 🟡 Improved | 3× supersampled anti-aliasing (budget-capped) for fills/strokes/glyphs; base-14 text renders real letterforms from a substitute system font (`PDFRS_UNICODE_FONT_PATH` or well-known paths) with base-14 width tables. Pixel-perfect parity with PDFium/Ghostscript remains out of scope by design (see README limitations). |

**Verification:** `cargo test` 508 passed / 0 failed; `cargo clippy --all-targets` 0 warnings; `cargo fmt --check` clean.

---

## Remediation Summary (2026-08-20)

| Finding | Status | Resolution |
|---------|--------|------------|
| C1 cosmetic image redaction | ✅ Fixed | `redact.rs` drops unreferenced image objects; resource entries removed; regression test |
| C2 deterministic non-spec encryption | ✅ Fixed | Full rewrite: Alg 2/3.3/3.4/3.5, R6 (2.A-2.F incl. 2.B hardened hash), `getrandom`; tests |
| C3 lossy-offset encrypt corruption | ✅ Fixed | Byte-precise scanner, `/Length`-verified streams, xref/trailer rebuild; objstm/xref-stream rejected |
| H1 non-conformant encrypted output | ✅ Fixed | Fresh xref table, correct `startxref`/`%%EOF`, dynamic object numbering |
| H2 redaction bypass (outside BT…ET, forms, annots) | ✅ Fixed | All text-showing ops masked; Form XObjects + `/AP` streams rewritten |
| H3 no CI | ✅ Fixed | `.github/workflows/{ci,audit,release}.yml` restored; clippy now strict (`-D warnings`) |
| H4 30 uncached regexes | ✅ Fixed | 26 prod sites migrated to `OnceLock` via per-module `*_regex!` macros |
| H5 duplicated utilities | ✅ Fixed | `decompress_stream` unified (mod-31 zlib check); `collect_font_metrics` scans all objects; raster keeps its specialized glyph-metrics variant |
| H6 anyhow-only error model | 🟡 Deferred | 106 `anyhow!` sites remain; typed error enum is a breaking-API change — scheduled as its own PR |
| M1 `from_utf8_lossy` 79 sites | 🟡 Scoped | Offset-splicing misuse fixed (C3, sign path). Remaining uses are display/parse fallbacks where lossy is correct; policy documented |
| M2 unclamped raster allocation | ✅ Fixed | 32,768 px clamp per dimension |
| M3 `/W` range DoS | ✅ Fixed | 65,535-glyph range bound |
| M4 unbounded HTML recursion | ✅ Fixed | 128-depth cap + iterative flatten salvage; 5000-deep test |
| M5 rc4 empty-key panic | ✅ Fixed | Returns error |
| M6 API hardening | ✅ Fixed | Body limit (413 test), `spawn_blocking`, opt-in CORS, `serve` returns Result |
| M7 redact `rfind('/')` truncation | ✅ Fixed | Explicit name-operand tracking; regression test |
| M8 god modules | 🟡 Deferred | `pdf.rs` 3,041 / `raster.rs` / `content_stream.rs` splits need dedicated refactors |
| M9 `main.rs` monolith | 🟡 Deferred | 2,113 LOC; split planned alongside M8 |
| L1 4 clippy warnings | ✅ Fixed | 0 warnings across `--all-targets` (default + `api`) |
| L2 `api::serve` unwrap/exit | ✅ Fixed | Returns `anyhow::Result<()>` |
| L3 `sig_obj_num = 999` + `%%EOF` panic | ✅ Fixed | Dynamic numbering; byte-level fixed-width splice |
| L4 WASM main-thread blocking | 🟡 Deferred | Requires JS-side worker wiring (`wasm/worker.js` already provides it); making the exported fns async would break the public API |
| L5 no RNG / cargo-audit | ✅ Fixed | `getrandom` added (wasm-gated); `audit.yml` runs cargo-audit |

**Verification:** `cargo test` 487 passed / 0 failed; `cargo clippy --all-targets` 0 warnings (default and `api`); `cargo fmt --check` clean; `--no-default-features`, `--features wasm` (wasm32), and `--features api` builds green.


---

## Archived: 2026-08-18 Audit Detail

## 1. Status of 2026-08-15 Audit Items

| ID | Issue | Status | Evidence |
|----|-------|--------|----------|
| C1 | Image redaction cosmetic | ❌ Open | `redact.rs:295-336` — `Do` operator removed but XObject stream survives in `doc.to_bytes()` at line 146 |
| C2 | Deterministic non-spec encryption | ❌ Open | `security.rs:546-556` — still `SHA-256(user_pw ‖ "pdfrs-aes256-key-derivation")`, no RNG dep added |
| C3 | Lossy-offset corruption in `encrypt_pdf_bytes` | ❌ Open | `pdf_ops/security.rs:76` — still `String::from_utf8_lossy(pdf_bytes)` + splice into original bytes |
| H1 | Encrypted output non-conformant | ❌ Open | `pdf_ops/security.rs:338` — `unwrap_or(0)` for startxref; `sig_obj_num = 999` at line 371 |
| H2 | Redaction bypass outside BT…ET | ❌ Open | `redact.rs:402` — `if in_text {…} else { original }` still passes text through unmasked |
| H3 | No CI | ❌ Open | No `.github/` directory exists |
| H4 | Uncached hot-path regexes | ❌ Worse | 30 `Regex::new` sites (was 23). `OnceLock` pattern exists in 5 files (`pdf.rs`, `elements.rs`, `code_highlight.rs`, `math_layout.rs`, `text_support.rs`) but not applied to the 30 violating sites |
| H5 | Duplicated utilities | ❌ Open | `decompress_stream` in 5 files, `collect_font_metrics` in 4 files — unchanged |
| H6 | `anyhow`-only error model | ❌ Open | 106 `anyhow!` sites — unchanged |
| M1 | `from_utf8_lossy` proliferation | ❌ Open | 79 sites — unchanged |
| M2 | Unclamped raster allocation | ❌ Open | `raster.rs:87-88` — no upper bound on `width_px`/`height_px` |
| M3 | `/W` range DoS | ❌ Open | `raster.rs:846-848` — `c_first..=c_last` with no bound |
| M4 | Unbounded HTML recursion | ❌ Open | `html.rs:599` `convert_node` recurses without depth limit; `collect_text_inner` at line 939 also unbounded |
| M5 | `rc4_encrypt` empty-key panic | ❌ Open | `security.rs:460` — `key[i % key.len()]` panics on empty key |
| M6 | API hardening | ❌ Open | `api.rs:318` `CorsLayer::permissive()`; `max_body` dead (no `DefaultBodyLimit`); no `spawn_blocking` in `api.rs` |
| M7 | Redact `rfind('/')` truncation | ❌ Open | `redact.rs:322` — unchanged |
| M8 | God modules | ❌ Open | `pdf.rs` 3,050; `raster.rs` 2,579; `content_stream.rs` 2,416; `main.rs` 2,113; `vector.rs` 2,086 — unchanged |
| M9 | `main.rs` monolith | ❌ Open | 2,113 LOC; `fn main()` spans ~1,456 lines — unchanged |
| L1 | 4 clippy warnings | ❓ Unverified | Could not run clippy (no network). Code patterns at cited locations unchanged |
| L2 | `api::serve` unwrap/exit | ❌ Open | `api.rs:332-337` — unchanged |
| L3 | `sig_obj_num = 999` + `%%EOF` panic | ❌ Open | `pdf_ops/security.rs:371, 338` — unchanged |
| L4 | WASM main-thread blocking | ❌ Open | `wasm.rs:64` — synchronous `pub fn` |
| L5 | No RNG / cargo-audit | ❌ Open | No `rand`/`getrandom` in Cargo.toml |

**Net:** 1 item fixed (`#[allow(dead_code)]` cleanup); 1 item regressed (uncached regexes 23 → 30); 21 items unchanged.

---

## 2. Critical Findings (Unchanged from 2026-08-15)

### C1: Image Redaction Is Cosmetic — Image Bytes Survive

**Location:** `src/redact.rs:295-336`, `src/redact.rs:146`

The `Do`-operator handler removes `/Name Do` from the content stream and emits `% redacted image` — but **never removes or alters the image XObject stream object itself**. Only page `/Contents` streams are mutated (redact.rs:103-143); all objects in `doc.objects`, including the "redacted" image, are re-serialized verbatim by `doc.to_bytes()` (line 146). Any extractor (including pdfrs itself) recovers the image unchanged.

**Fix:** after rewriting content streams, walk each page's `/Resources /XObject` dict, drop image objects no longer referenced by any content stream (or replace their stream with an empty/gray stream and update `/Length`), then remove them from `doc.objects` and rebuild the xref.

### C2: Encryption Core Is Deterministic and Not PDF-Spec Conformant

**Locations:** `src/security.rs:321-325, 342-393, 499-571`

- **AES-256 file key = `SHA-256(user_pw ‖ "pdfrs-aes256-key-derivation")`** (security.rs:546-556) — no random 32-byte file key, no salts, no hardening iteration. Same password ⇒ identical key for every document.
- **`/U` uses `file_key[0..8]` as its own salt** (security.rs:364-371) — pure function of password; owner password and flags ignored.
- **Deterministic CBC IV = `SHA-256(key‖plaintext)[..16]`** (security.rs:559-571) — IV predictable; identical inputs ⇒ identical ciphertext.
- **Algorithm 2 omits mandatory trailer `/ID`** (security.rs:499-544) — non-compliant with PDF 1.7 Alg 2 step 4.
- **`/O`/`/U` computed non-standard** (security.rs:342-359, 362-393) — Acrobat/mupdf/qpdf will not validate.
- **No RNG dependency** (`rand`/`getrandom` absent from Cargo.toml) — root cause.

**Fix:** add `getrandom`; implement Algorithm 2/2.B per spec with random file key + salts + `/ID`; store random IV for CBC.

### C3: `encrypt_pdf_bytes` Corrupts or Panics on Real PDFs

**Location:** `src/pdf_ops/security.rs:76-141`

Regex surgery runs over `String::from_utf8_lossy(pdf_bytes)` (line 76), but match offsets slice the **original** `pdf_bytes` (line 101). Binary PDFs (Flate streams — `0x78 0x9C` is not UTF-8) make lossy-string offsets diverge from byte offsets ⇒ out-of-bounds panic or wrong-byte splicing. Stream data encrypted at line 119 is lossy-mangled text. Same pattern repeats at lines 145-151, 165, 169-191.

**Fix:** operate on bytes end-to-end: `regex::bytes::Regex` over `&pdf_bytes[..]`, splice by byte offsets, encrypt original stream bytes.

---

## 3. High Findings (Unchanged from 2026-08-15)

### H1: Encrypted Output Is Structurally Non-Conformant

**Locations:** `src/pdf_ops/security.rs:143-195, 338-343, 371`

- `/Encrypt` object inserted before xref but `startxref` never re-pointed; new object gets no xref entry.
- Missing `%%EOF` ⇒ `startxref_pos = 0` ⇒ `&output[9..0]` slice panic (lines 338-343).
- `sig_obj_num = 999` (line 371) — collides with documents having ≥1000 objects.

### H2: Redaction Bypasses Beyond Page Content Streams

**Locations:** `src/redact.rs:103-143, 411-416, 434-437, 461-463`

- Text-showing operators **outside `BT…ET`** pass through unmasked (`if in_text {…} else { original }`).
- Text inside **Form XObjects** and **annotation appearance streams** is never touched.

### H3: No CI

No `.github/` directory exists. Nothing guards fmt/clippy/tests/advisories. **Still the single highest-leverage fix.**

### H4: 30 Uncached Hot-Path Regexes (was 23)

30 production `Regex::new` sites remain. The `OnceLock`/`pdf_regex!` pattern is established in 5 files (`pdf.rs`, `elements.rs`, `pdf_generator/code_highlight.rs`, `pdf_generator/math_layout.rs`, `pdf_generator/text_support.rs`) but not applied to the 30 violating sites across `pdf_ops/forms.rs` (4), `pdf_ops/structure.rs` (5), `pdf_ops/tables.rs` (4), `pdf_ops/security.rs` (4), `vector.rs` (3), `incremental.rs` (2), `cli_repl.rs` (1), `pdf.rs` (2), `pdf_generator/math_layout.rs` (2), `pdf_generator/text_support.rs` (1), `elements.rs` (1), `linearize.rs` (1). Migration is mechanical.

### H5: Duplicated Utilities (Unchanged)

`decompress_stream`: 5 files (`pdf.rs`, `search.rs`, `raster.rs`, `redact.rs`, `pdf_to_md.rs`) — copies disagree on zlib detection. `collect_font_metrics`: 4 files (`search.rs`, `raster.rs`, `redact.rs`, `pdf_to_md.rs`). ~100+ LOC of pure deletion available.

### H6: `anyhow`-Only Error Model (Unchanged)

106 `anyhow!` sites; library consumers cannot match error variants. `PdfError` facade (+ `thiserror`) recommended.

---

## 4. Medium Findings (Unchanged from 2026-08-15)

| ID | Finding | Location |
|----|---------|----------|
| M1 | `from_utf8_lossy`: 79 sites; top: `pdf.rs` (18), `pdf_ops/security.rs` (7), `raster.rs` (7) | various |
| M2 | Raster allocation unclamped: `/MediaBox` × DPI ⇒ OOM (`[0 0 100000 100000]` @300dpi) | `raster.rs:87-88` |
| M3 | `/W` range `[0 4294967295 500]` ⇒ ~4×10⁹ HashMap inserts | `raster.rs:846-848` |
| M4 | Unbounded recursion in HTML→PDF (`<div>`×100k ⇒ stack overflow) | `html.rs:599, 784-799, 939-953` |
| M5 | `rc4_encrypt` panics on empty key (`key[i % key.len()]`); `pub` fn | `security.rs:460` |
| M6 | API: `CorsLayer::permissive()`; `max_body` dead; no `spawn_blocking` | `api.rs:42-51, 318` |
| M7 | Redact `out.rfind('/')` + truncate — truncates unrelated output | `redact.rs:322-324` |
| M8 | God modules: `pdf.rs` 3,050; `raster.rs` 2,579; `content_stream.rs` 2,416; `main.rs` 2,113; `vector.rs` 2,086 | various |
| M9 | `main.rs` monolith — 49 subcommand variants in ~1,456-line `fn main()` | `main.rs:658-2113` |

---

## 5. Low Findings (Unchanged from 2026-08-15)

| ID | Finding | Location |
|----|---------|----------|
| L1 | 4 clippy warnings (unverified — could not run clippy) | `redact.rs:98,299`; `security.rs:267,455` |
| L2 | `api::serve`: `.unwrap()` + `std::process::exit(1)` | `api.rs:332-337` |
| L3 | `sig_obj_num = 999` collision; `%%EOF`-missing slice panic | `pdf_ops/security.rs:371, 338` |
| L4 | WASM export blocks caller thread | `wasm.rs:64` |
| L5 | No `cargo audit` baseline; no RNG dependency (root cause of C2) | `Cargo.toml` |

---

## 6. Hygiene Detail

- **`unsafe`**: 0 sites — clean.
- **Production `panic!`**: 0 — all 32 `panic!` matches are in test modules only.
- **`todo!`/`unimplemented!`/TODO/FIXME**: 0 — clean.
- **`#[allow(dead_code)]`**: 0 — **improved** (was 3 in previous audit).
- **`unwrap()`**: 493 matches across 40 files (was ~490). Most are in test code or on infallible operations; production hotspots: `parallel.rs` `to_str().unwrap()` on paths, `api.rs:337` serve unwrap.
- **`from_utf8_lossy`**: 79 sites — unchanged.
- **`anyhow!`**: 106 sites — unchanged.
- **`Regex::new`**: 30 uncached sites — up from 23.
- **`OnceLock`**: 5 files use the cached pattern correctly (`pdf.rs`, `elements.rs`, `code_highlight.rs`, `math_layout.rs`, `text_support.rs`).
- **RC4** (`security.rs:453-474`): textbook-correct KSA/PRGA, no drop-256 (correct for PDF interop). No constant-time comparison (moot until password-verify API exists).
- **Positives**: `validate_cert_id()` guard consistent across `CertificateStore`; `async_api.rs` exemplary (`tokio::fs` + `spawn_blocking`, no unwraps); ttf-parser integration fully fallible; `chart.rs` and CSS parser panic-free.

---

## 7. Documentation Drift ("Docs are code" violations)

1. **Version incoherence**: Cargo.toml = `0.1.10`; CHANGELOG has no 0.1.10 entry (all work under `[Unreleased]`); README.md:192 tells users to install `pdfrs = "0.2"` (wrong on two counts — version is 0.1.10, not 0.2).
2. **Removed CI still documented**: README CI badge (line 18), CHANGELOG:76-84 (lists 3 workflows as "Added"), TODO.md:311,330-333 (marked `[x]`) — `.github/` does not exist.
3. **README stale on shipped features**: line 263 "stub crypto gated" (real RC4/AES shipped); line 296 "Rasterizer is schematic: text glyphs render as gray rectangles" (glyph-outline rasterization from embedded TTF shipped per CHANGELOG); line 301 "no stacked or multi-series charts yet" (`StackedBar` shipped); line 302 "Full tagged PDF output not yet implemented" (tagged PDF generation shipped per TODO.md). REST API and CSS support absent from README feature list. `protect` undocumented.
4. **SPEC self-contradiction**: SPEC.md:485-493 "Remaining Features" lists encryption ("crypto currently gated to refuse fake protection"), partial-string redaction, image XObject removal, and font-outline rasterization as *not done* — all four shipped per CHANGELOG/TODO.
5. **ARCHITECTURE gaps**: no sections for `async_api.rs`, `cli_repl.rs`, `rtl.rs`, `table_renderer.rs`, `builder.rs`, `parallel.rs`.
6. **Stale test count**: README:269 says "~395 tests"; TODO.md:399 says "455 passing tests"; previous audit found 473. Actual count unknown (could not run tests).
7. **Examples frozen**: no examples cover v0.1.9+ features (encryption, API, redact, search, raster, stacked charts).
8. Minor: stray `output.pdf` at repo root; `USER_GUIDE.md` missing `create-portfolio`/`list-certificates`.

---

## 8. Recommended Fix Priority

1. **Fix or gate the encryption feature** (C2, C3, H1) — either implement spec-conformant Algorithm 2/2.B with a real RNG, or mark `protect`/`encrypt_pdf_bytes` experimental in README/CHANGELOG. Highest user-damage risk.
2. **Fix image redaction** (C1) — delete/neutralize XObject stream objects, not just the `Do` operator.
3. **Close redaction bypasses** (H2) — strip `BT`-gate asymmetry, walk Form XObjects and annotation appearance streams.
4. **Add CI** (H3) — fmt, clippy `-D warnings`, test matrix, `cargo audit`. Restores the safety net every other fix depends on.
5. **Deduplicate `decompress_stream`/`collect_font_metrics`** (H5) — pure deletion; resolves behavioral divergence.
6. **Migrate 30 uncached regexes to `OnceLock`** (H4) — mechanical, pattern already established in 5 files.
7. **API hardening** (M6) — `DefaultBodyLimit`, configurable CORS, `spawn_blocking` (copy from `async_api.rs`).
8. **DoS guards** (M2-M4) — clamp raster dimensions, bound `/W` ranges, add DOM-depth limit to HTML conversion.
9. **Docs sync pass** (§7) — reconcile version, purge dead CI claims, fix stale feature claims, one canonical test count, update SPEC "Remaining Features".
10. **Structural**: split `main.rs` into `cli/` (M9), add `PdfError` facade (H6), decompose `pdf.rs`/`raster.rs`/`content_stream.rs` (M8), tackle `from_utf8_lossy` (M1), add missing doctests and examples.

---

## Appendix: Methodology

- **Source-level review only** — `cargo clippy` and `cargo test` could not run (no network access for crate downloads; `--offline` failed on missing cached `aes v0.8.4`).
- Direct source review of all critical-path modules: `security.rs`, `pdf_ops/security.rs`, `redact.rs`, `api.rs`, `raster.rs`, `html.rs`, `wasm.rs`, `pdf_generator/content_stream.rs`.
- Pattern greps: `unwrap(`, `panic!`, `unsafe`, `todo!`/`unimplemented!`/FIXME/TODO/HACK/XXX, `Regex::new`, `from_utf8_lossy`, `#[allow(dead_code)]`, `OnceLock`, `anyhow!`, `rand`/`getrandom`, `CorsLayer::permissive`, `spawn_blocking`, `DefaultBodyLimit`, `decompress_stream`, `collect_font_metrics`.
- LOC ranking via `wc -l` across all 48 src files.
- Docs-vs-code cross-check: README/CHANGELOG/TODO/SPEC/ARCHITECTURE vs `lib.rs` module list and `main.rs` command enum.
- Not measured: clippy warnings, test count, runtime benchmarks, memory profile, WASM build, `cargo audit`.

Comparison against 2026-08-15 AUDIT.md: **1 of 23 open items fixed** (`#[allow(dead_code)]` cleanup); **1 regressed** (uncached regexes 23 → 30); 21 unchanged. 3 Critical findings remain unresolved through 2 version bumps.
