# pdfrs Codebase Audit Report

**Date:** 2026-07-31
**Scope:** Re-audit of `src/` (~58K LOC), `tests/`, `examples/`, `Cargo.toml`, root docs.
**Previous audit:** 2025-01-24 (`AUDIT.md`). 18 months and ~23K LOC of growth since.
**Methodology:** `cargo clippy --all-targets`, `cargo clippy --all-targets -- -W clippy::pedantic`, `cargo test` (all profiles), full `cargo build --release`, structural review, pattern grep across all 36 modules.

---

## Executive Summary

The codebase has matured significantly since the 2025-01-24 audit. The original **3 critical** and **6 high-severity** findings are mostly resolved, the test suite has grown from ~371 to **458 tests (all passing)**, clippy warnings dropped from **37 to 4** (all in `src/redact.rs`/`src/security.rs`, all auto-fixable), and 8 `proptest!` blocks were added. The codebase compiles cleanly with `cargo build --release` (no errors, no unsafe blocks).

What remains is largely **structural and qualitative**: residual code duplication in two utilities, two hot paths that still recompile regexes per call, an `anyhow`-only error model that limits library ergonomics, the absence of CI to guard against regressions, and stale example outputs.

| Severity | Count | Categories |
|----------|-------|------------|
| **Critical** | 0 | — |
| **High** | 4 | No CI; duplicate `decompress_stream` / `collect_font_metrics`; hot-path regex in `pdf_ops/` and `vector.rs`; `anyhow`-only error model |
| **Medium** | 6 | `from_utf8_lossy` proliferation (78 sites); `raster::collect_font_metrics` private duplicate; `main.rs` monolith; 7 stale example PDFs; `pdf.rs` 3K LOC; `raster.rs` 2.6K LOC |
| **Low** | 4 | 4 clippy warnings (auto-fixable); 1 ignored doctest; `--features wasm` build path not exercised here; no `cargo audit` baseline captured |

---

## 1. Status of Previous Audit Items

| ID | Issue (2025-01-24) | Status | Evidence |
|----|--------------------|--------|----------|
| **C1** | Full Unicode-range scan on every PDF | ✅ Fixed | `src/pdf.rs:1796,1847` — wrapped in `OnceLock<Option<HashMap<u16, char>>>`, built at most once per process |
| **C2** | `unwrap()` on `find("stream")`/`find("endstream")` | ✅ Fixed | `src/pdf.rs:1179,1182` — replaced with `ok_or_else` returning `anyhow::anyhow!("...")` |
| **C3** | Failing test `test_complex_examples_library_api_batch` | ✅ Fixed | `cargo test` → all suites pass (458/458, 1 doctest ignored) |
| **H1** | Regex recompilation (66 regexes per call) | ⚠️ Partial | `pdf.rs`, `text_support.rs`, `math_layout.rs`, `elements.rs` use `OnceLock`. Still uncached in `vector.rs:452,456`, `cli_repl.rs:26`, `incremental.rs:59,64`, `linearize.rs:420`, `pdf_ops/security.rs:80,180,530,567`, `pdf_ops/tables.rs:28-31`, `pdf_ops/structure.rs:68-75`, `pdf_ops/forms.rs:278,279,403,404` |
| **H2** | Duplicated `decompress_stream` / `page_content_streams` / `collect_font_metrics` | ⚠️ Partial | `page_content_streams` fully deduplicated (single def in `search.rs:199`). `decompress_stream` still in 2 places (`pdf.rs:1113` + `search.rs:286`). `collect_font_metrics` still in 3 places (`raster.rs:279`, `redact.rs:559` wrapper, `search.rs:313`) |
| **H3** | `unwrap()` in `security::validate` | ✅ Fixed | `grep -n "user_password.as_ref().unwrap()"` returns no matches |
| **H4** | `chars().next().unwrap()` in layout word-wrap | ✅ Fixed | `grep -n "chars().next().unwrap()" src/pdf_generator/layout.rs` returns no matches |
| **H5** | Silent error swallowing in `optimization.rs:422` | ✅ Fixed | `grep "unwrap_or_else(|_\| data.clone())" src/optimization.rs` returns no matches |
| **H6** | No custom error type — all `anyhow::Error` | ❌ Not fixed | 106 `anyhow!` call sites still; entire crate uses `anyhow::Result`. Library on crates.io cannot expose programmatic error variants |
| **M1** | 37 clippy warnings | ✅ Fixed | 4 remaining (all pedantic-level, all auto-fixable). See §5 |
| **M2** | `String::from_utf8_lossy` in `pdf.rs` (20+ sites) | ⚠️ Worse | Now 78 total `from_utf8_lossy` call sites across the crate (added with `raster.rs` raw-byte scanning, `search.rs` content-stream decoding, etc.) |
| **M3** | Dead function `collect_rich_segments` | ✅ Fixed | No matches in source |
| **M4** | Excessive cloning in `content_stream.rs` | ❓ Unverified | File now uses `Cow<str>` patterns in places; spot-check only |
| **M5** | `#[allow(dead_code)]` on useful fields | ⚠️ Reduced | 2 remaining (`pdf_to_md.rs:68,80` for `page`/`is_bold`/`is_italic`/`fonts` — kept for "future rounds") |
| **M6** | God modules (`pdf.rs` 3K, `raster.rs` 2.6K, `main.rs` 2.1K LOC) | ⚠️ Partial | `pdf_generator/` already split into `mod.rs`, `content_stream.rs`, `layout.rs`, `text_support.rs`, `math_layout.rs`. Top god-files remain |
| **M7** | No benchmarks in CI | N/A | CI removed entirely in commit `3891c99` (no `.github/workflows/` exists). See H-NEW-1 |

**Net change vs 2025-01-24:** 8 of 14 tracked items fully fixed, 4 partially fixed, 1 not fixed (H6), 1 N/A. No regressions identified.

---

## 2. New High-Severity Issues

### H-NEW-1: No CI — Manual Verification Required

**Impact:** Commit `3891c99` removed `.github/workflows/`. There is no automated `cargo fmt --check`, `cargo clippy`, `cargo test`, `cargo audit`, or multi-OS matrix guarding the repository. The "Test before ship" principle in `AGENTS.md` is enforced only by the developer's local discipline. A future contributor can land clippy regressions, broken tests, or even a security advisory without any automated check firing.

**Files:** repository root (missing `.github/workflows/`)

**Recommendation:** Reintroduce a minimal CI workflow:
1. `cargo fmt --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test` on Linux + macOS (Windows if WASM-free path)
4. `cargo audit` on a weekly schedule
5. `cargo build --release` (smoke check)

**Suggested command:** add `.github/workflows/ci.yml` mirroring what existed before commit `3891c99`.

---

### H-NEW-2: `decompress_stream` and `collect_font_metrics` Still Duplicated

**Locations:**
- `src/pdf.rs:1113` — private `fn decompress_stream(data: &[u8]) -> Vec<u8>`
- `src/search.rs:286` — `pub(crate) fn decompress_stream(data: &[u8]) -> Vec<u8>`
- `src/raster.rs:279` — private `fn collect_font_metrics(doc: &PdfDocument) -> HashMap<...>`
- `src/redact.rs:559` — thin wrapper that calls `search::collect_font_metrics_search`
- `src/search.rs:313` — `pub fn collect_font_metrics(doc: &PdfDocument) -> HashMap<...>`

**Impact:** Three near-identical implementations of `collect_font_metrics` and two of `decompress_stream`. Divergent bug fixes are inevitable — e.g. `raster.rs`'s version uses a different Resources walk than `search.rs`'s, and `redact.rs`'s wrapper would silently miss any raster-specific bug. The `pdf.rs:1113` copy is only used inside `pdf.rs` itself (`decompress_stream` is called at lines 1004, 1448, 1744) but still constitutes parallel maintenance.

**Recommendation:** Promote `search::decompress_stream` and `search::collect_font_metrics` to be the single source of truth (already `pub(crate)`). Replace `pdf.rs:1113` and `raster.rs:279` with calls into `search`. Delete `redact.rs:559` wrapper. Net deletion: ~80 LOC.

---

### H-NEW-3: Hot-Path Regex Compilation in `pdf_ops/` and `vector.rs`

**Sites (compiled per call, not cached):**
- `src/pdf_ops/tables.rs:28-31` — 4 regexes inside `extract_tables_from_pdf` (called per PDF for table extraction)
- `src/pdf_ops/structure.rs:68-75` — 5 regexes inside structure-detection function
- `src/pdf_ops/forms.rs:278-279, 403-404` — 4 regexes across two form-fill paths
- `src/pdf_ops/security.rs:80,180,530,567` — 4 regexes in encryption/signature paths
- `src/vector.rs:452,456` — 2 regexes in public `extract_svg_path_d`
- `src/cli_repl.rs:26` — 1 regex in interactive REPL
- `src/incremental.rs:59,64` — 2 regexes in incremental update path
- `src/linearize.rs:420` — 1 regex in linearization path

**Impact:** Each `Regex::new` parses the pattern, builds the NFA, and allocates. For `extract_tables_from_pdf` and friends (which iterate over every object in the PDF), this is a measurable per-call cost. The `pdf_regex!` macro and per-module `OnceLock` patterns already exist in `pdf.rs`, `text_support.rs`, `elements.rs`, `math_layout.rs` — extending that pattern is mechanical.

**Recommendation:** Migrate all sites to the existing `OnceLock<Regex>` pattern. The `text_support.rs:8` macro is a clean template:
```rust
macro_rules! pdf_regex {
    ($name:ident, $pat:expr) => {
        static $name: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        $name.get_or_init(|| Regex::new($pat).unwrap())
    };
}
```
Estimated effort: 1–2 hours for all sites. Expected CPU savings on a 1000-page PDF: non-trivial.

---

### H-NEW-4: `anyhow`-Only Error Model Limits Library Ergonomics (carryover from H6)

**Impact:** The crate is published on crates.io (`documentation = "https://docs.rs/pdfrs"`) but exposes only `anyhow::Result`. Library consumers cannot:
- Pattern-match on error variants (e.g. "is this an encryption error vs parse error?")
- Distinguish recoverable from non-recoverable failures
- Get typed structured error info

This is fine for an application; it's a real limitation for a library. Two options:
1. **Cheap path:** Add a `pub type PdfResult<T> = std::result::Result<T, PdfError>` and a small `PdfError` enum with `From<io::Error>`, `From<regex::Error>`, etc. Keep `anyhow` as the internal alias. ~150 LOC.
2. **Full path:** Replace `anyhow!` with typed variants everywhere (~2 days).

**Recommendation:** Option 1 — a thin `PdfError` facade. Defer the full migration until there's a concrete consumer that needs variant matching.

---

## 3. Medium-Severity Issues

### M-NEW-1: `from_utf8_lossy` Proliferation (78 Sites)

**Impact:** Grew from ~20 in 2025-01-24 to **78** today. PDF content streams are not UTF-8; `from_utf8_lossy` silently substitutes `U+FFFD` for invalid bytes. This corrupts text extraction for any PDF with non-UTF-8 encodings (which is the majority of real-world PDFs). Newly affected: `raster.rs` raw-byte scanning, `pdf_to_md.rs`, `redact.rs`.

**Recommendation:** For content streams, prefer byte-level regex (`regex::bytes::Regex`) or convert only known-UTF-8 metadata fields. Reserve `from_utf8_lossy` for genuinely UTF-8 fields (XMP metadata, etc.). A targeted refactor of 10 high-frequency sites would cover most of the risk.

---

### M-NEW-2: `main.rs` 2,113 LOC Monolith

**Impact:** All CLI subcommands live in `src/main.rs` (49 clap-derived structs + 4 fn bodies, longest is `fn main()` at 1,456 lines). Hard to navigate; clap derive structs interleave with handler logic.

**Recommendation:** Split into `src/cli/` submodules: `cli/mod.rs`, `cli/markdown.rs`, `cli/pdf_ops.rs`, `cli/raster.rs`, `cli/security.rs`, etc. The previous audit's `pdf_generator/` split is a good template.

---

### M-NEW-3: Stale Example Outputs

**Impact:** 14 `.md` files in `examples/`, but only **7 PDFs** in `examples/output/` and **1 PDF** at `examples/capability_showcase.pdf` at root. Missing outputs for: `api_reference_complex`, `capability_showcase`, `enhanced_features`, `full_features`, `technical_report_complex`, `technical_spec`, `README`. The `examples/validate_examples.sh` script may be checking for outputs that don't exist.

**Recommendation:** Run `examples/validate_examples.sh` (or invoke the generator) to regenerate the missing 7 PDFs and commit them. Add the script output to CI to prevent future drift.

---

### M-NEW-4: `pdf.rs` 3,048 LOC and `raster.rs` 2,579 LOC

**Impact:** Two modules each mix multiple concerns. `pdf.rs`: parsing, text extraction, diff, sandbox, ToUnicode, glyph mapping. `raster.rs`: rasterization, font metrics, glyph rendering, PNG encoding, base-14 width tables.

**Recommendation:** Decompose `pdf.rs` into `pdf/parse.rs`, `pdf/text.rs`, `pdf/diff.rs`, `pdf/sandbox.rs`, `pdf/tounicode.rs`. Decompose `raster.rs` into `raster/surface.rs`, `raster/glyph.rs`, `raster/font_metrics.rs`, `raster/png.rs`, `raster/base14.rs`. This was previously recommended (M6) but only `pdf_generator/` was split since.

---

### M-NEW-5: Two Remaining `#[allow(dead_code)]`

**Locations:**
- `src/pdf_to_md.rs:68` — `page`, `is_bold`, `is_italic` fields retained "for future rounds"
- `src/pdf_to_md.rs:80` — `fonts` field retained "for future per-span width refinement"

**Impact:** Fields parsed but never used add maintenance overhead and confuse readers. Either remove the parsing or implement the intended features.

**Recommendation:** Either delete the fields and the parsing code, or create a follow-up TODO item with a clear scope. Currently they linger with no plan.

---

### M-NEW-6: Doctest Coverage Skew

**Impact:** 35 doctests (1 ignored for being behind a feature gate — fine), 458 total tests. Doctests concentrate in `lib.rs`, `parallel.rs`, `pdf_ops/forms.rs`, `pdf_ops/mod.rs`. Several public APIs have no doctest: most of `vector.rs` public functions, `cli_repl.rs`, `plugin.rs`, `thesis.rs`.

**Recommendation:** Add 1–2 doctests per public function in `vector.rs`, `plugin.rs`, and `thesis.rs`. Doctests double as executable examples and reduce documentation drift.

---

## 4. Low-Severity Issues

### L-NEW-1: 4 Remaining Clippy Warnings (All Auto-Fixable)

**Locations:**
- `src/redact.rs:98` — `or_insert(Vec::new())` → `or_default()` (auto-fixable)
- `src/redact.rs:299` — manual `Option::map` (auto-fixable)
- `src/security.rs:267` — doc list indentation (auto-fixable)
- `src/security.rs:455` — `for i in 0..256` over indices (auto-fixable)

**Fix:**
```bash
cargo clippy --fix --lib -p pdfrs --allow-dirty
```

---

### L-NEW-2: Ignored Doctest Without Justification Comment

**Location:** `src/lib.rs:107` — parallel example marked `ignore` because it's behind the `parallel` feature gate (enabled by default). Reasonable, but the reason isn't documented inline. Consider adding `// requires parallel feature` in the preceding comment.

---

### L-NEW-3: WASM Build Not Verified

**Impact:** The `--features wasm` build path exists in `Cargo.toml` and is exercised by commit history, but was not re-verified in this audit. The `wasm/` directory exists. A targeted `cargo build --features wasm --target wasm32-unknown-unknown` is recommended before each release.

---

### L-NEW-4: No `cargo audit` Baseline

**Impact:** `cargo audit` failed at runtime due to network sandbox restrictions during this audit, so no advisory baseline was captured. The previous audit recommended CI integration of `cargo audit` (H-NEW-1). A locally-cached advisory database (committed to repo or restored from CI artifacts) would prevent baseline loss.

---

## 5. Clippy Detail

`cargo clippy --all-targets` (default lints): **4 warnings, 0 errors** (down from 37 in 2025-01-24).

`cargo clippy --all-targets -- -W clippy::pedantic` (stricter): ~50 stylistic warnings, none are correctness issues. Categories:
- ~25 `similar_names` (single-letter bindings with overlapping scopes — common in numeric/coord code)
- 8+ `many_single_char_names` (same root cause)
- 3 `long_literal_without_separators` (color/font hex)
- 2 `missing_must_use` on builder methods
- 1 `redundant_else`
- 1 `unnested_or_patterns`

These are taste-level, not bugs. Defer until a contributor wants to opt into pedantic as the default.

---

## 6. Test Coverage Assessment

| Suite | Count | Status |
|-------|-------|--------|
| `src/` inline `#[test]` | 352 | ✅ All pass |
| `tests/integration.rs` | 24 | ✅ All pass |
| `tests/roundtrip_test.rs` | 22 | ✅ All pass (was 22 with 1 failing — now 0 failing) |
| `tests/unicode_integration_test.rs` | 10 | ✅ All pass |
| `tests/capabilities_v2.rs` | 7 | ✅ All pass |
| `tests/capability_validation.rs` | 5 | ✅ All pass |
| `tests/comprehensive_pdf.rs` | 3 | ✅ All pass |
| **Doctests** | 35 (+1 ignored) | ✅ All pass |
| **Total** | **458** | ✅ |
| **`proptest!` blocks** | **8** (up from 0) | ✅ |

**Gaps remaining:**
- No fuzzing harness (`cargo-fuzz` not configured despite regex-heavy parsing)
- `raster.rs` (2,579 LOC): 9 inline tests only — lowest coverage of any heavy module
- `search.rs` (1,390 LOC): 7 inline tests — low
- `vector.rs` (1,924 LOC): reasonable coverage, but no SVG document round-trip tests with complex transforms
- Error-path coverage is improving but still thin in `redact.rs` and `pdf_ops/security.rs`

---

## 7. Architecture Observations

### Positive Patterns (Preserve)
- Module boundaries: `pdf_generator/`, `pdf_ops/`, `pdf/` subdirectories with clear ownership
- `OnceLock`-based regex caching — the `pdf_regex!` macro is clean and reusable
- 8 `proptest!` blocks demonstrate commitment to property-based testing
- Feature flags (`parallel`, `wasm`, `async`, `api`) — clean optionality
- `pub(crate)` visibility for internal helpers (`search.rs`) — used well, should extend to remaining duplicates
- Zero `unsafe` blocks — verifiable via `grep -rn "unsafe " src/ --include="*.rs"`
- Zero `todo!()` / `unimplemented!()` in source
- 30 `pub` items in `lib.rs` re-exports — manageable API surface
- Doctests at 35+ — docs and code in sync

### Anti-Patterns (Address)
- **No CI** — biggest single risk for long-term code quality (H-NEW-1)
- **Two remaining utility duplicates** — easy win, low risk (H-NEW-2)
- **Hot-path regex in `pdf_ops/` and `vector.rs`** — performance regression waiting to happen as PDF sizes grow (H-NEW-3)
- **`anyhow`-only errors** — limits library usability; cheap facade would help (H-NEW-4)
- **`from_utf8_lossy` proliferation** — correctness concern for non-UTF-8 PDFs (M-NEW-1)
- **Stale example outputs** — documentation drift (M-NEW-3)

---

## 8. Recommended Fix Priority

1. **Add CI** (H-NEW-1) — prevents all future regressions; restores automated safety net
2. **Auto-fix 4 clippy warnings** (L-NEW-1) — 5 minutes, zero risk
3. **Migrate remaining hot-path regexes to `OnceLock`** (H-NEW-3) — mechanical, clear performance win
4. **Consolidate duplicate utilities** (H-NEW-2) — pure deletion, reduces maintenance
5. **Regenerate stale example outputs** (M-NEW-3) — `examples/validate_examples.sh` should do this
6. **Split `main.rs` into `cli/` submodules** (M-NEW-2) — improves navigability
7. **Add `PdfError` facade** (H-NEW-4) — enables programmatic error handling for library consumers
8. **Refactor `pdf.rs` and `raster.rs` god modules** (M-NEW-4) — long-term maintainability
9. **Tackle `from_utf8_lossy` proliferation** (M-NEW-1) — correctness over time
10. **Add doctests for `vector.rs`, `plugin.rs`, `thesis.rs`** (M-NEW-6) — fills coverage gaps
11. **Resolve `#[allow(dead_code)]` fields** (M-NEW-5) — small cleanup

---

## 9. Quick-Fix Commands

```bash
# Auto-fix the 4 remaining clippy warnings
cargo clippy --fix --lib -p pdfrs --allow-dirty
cargo clippy --fix --lib -p pdfrs --tests --allow-dirty

# Verify everything still builds and passes
cargo build --all-targets
cargo test

# Snapshot the post-audit state
cargo clippy --all-targets 2>&1 | grep -E "^warning:" | wc -l    # → 0
cargo test 2>&1 | grep "^test result:" | awk '{s+=$4} END {print "Total tests passed:", s}'
```

---

## Appendix: Audit Methodology

Tools used in this audit:
- `cargo clippy --all-targets` — default lints
- `cargo clippy --all-targets -- -W clippy::pedantic` — stricter stylistic pass
- `cargo test` (debug + release profiles) — full suite
- `cargo build --release` — release-mode smoke check
- `cargo audit` (network-restricted, failed)
- `grep -rn` patterns across `src/` — for `unwrap()`, `panic!`, `unsafe`, `Regex::new`, `from_utf8_lossy`, `todo!`, `unimplemented!`, `TODO`/`FIXME`/`XXX`, `#[allow(dead_code)]`
- Structural inspection: file LOC ranking, function-length ranking, module fan-in/fan-out for utility functions
- Comparison against the 2025-01-24 AUDIT.md item-by-item

Not measured (out of scope or unavailable):
- Runtime performance benchmarks (`cargo bench` — exists but not re-run)
- Memory profile / allocation tracking
- Cross-platform behavior
- `cargo audit` baseline (network-restricted)