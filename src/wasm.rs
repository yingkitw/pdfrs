//! WebAssembly bindings for pdfrs
//!
//! This module exposes the PDF generation pipeline to JavaScript environments
//! via `wasm-bindgen`. All functions are pure in-memory — no filesystem access.
//!
//! # Build
//!
//! ```bash
//! wasm-pack build . --target web --out-dir wasm/pkg --features wasm --no-default-features
//! ```
//!
//! # Recommended: load WASM inside a Web Worker (worker-first)
//!
//! Generating PDFs is CPU-bound and blocks the main thread for the duration
//! of the call. For any non-trivial document, prefer the bundled
//! [`PdfWorkerClient`][worker_client] which loads this WASM module inside
//! a [`Worker`][worker] and exposes a Promise-based API:
//!
//! ```js
//! import { PdfWorkerClient } from './worker-client.js';
//!
//! const client = new PdfWorkerClient();
//! await client.init();
//! const pdfBytes = await client.render('# Big document...');
//! ```
//!
//! # Alternative: Promise-returning export on the main thread
//!
//! If running the WASM on the main thread is acceptable, the async
//! export integrates with `await` syntax without Worker setup:
//!
//! ```js
//! import init, { render_markdown_to_pdf_async } from './pkg/pdfrs.js';
//!
//! await init();
//! const pdfBytes = await render_markdown_to_pdf_async('# Hello WASM');
//! ```
//!
//! Both paths share the same underlying generator; only the threading
//! differs.
//!
//! # Synchronous export (small documents only)
//!
//! ```js
//! import init, { render_markdown_to_pdf } from './pkg/pdfrs.js';
//!
//! await init();
//! const pdfBytes = render_markdown_to_pdf("# Hello WASM\n\nIt works!");
//! // pdfBytes is a Uint8Array
//! ```
//!
//! This synchronous export blocks the main thread. Use it only for
//! tiny documents or inside the Worker (see `worker.js`).
//!
//! # IndexedDB caching
//!
//! The `cache.js` module caches the compiled WASM binary in IndexedDB
//! so subsequent page loads skip the network fetch:
//!
//! ```js
//! import { loadWasmWithCache } from './cache.js';
//! const wasm = await loadWasmWithCache('pkg/pdfrs_bg.wasm');
//! ```
//!
//! [worker]: https://developer.mozilla.org/en-US/docs/Web/API/Worker
//! [worker_client]: https://github.com/yingkitw/pdfrs/blob/main/wasm/worker-client.js

use wasm_bindgen::prelude::*;

/// Render Markdown to PDF bytes in a WebAssembly environment.
///
/// Takes a Markdown string and returns the raw PDF byte vector.
/// This function does **not** touch the filesystem — everything happens
/// in memory, making it ideal for browser or serverless WASM runtimes.
///
/// **Threading:** this is a synchronous export. Calling it from the
/// main browser thread will block the UI until generation completes.
/// For non-blocking behaviour, either:
///
/// - Call [`render_markdown_to_pdf_async`] (Promise-returning), which
///   lets callers `await` without explicit Worker plumbing, **or**
/// - Load the WASM module inside a [`Worker`][worker] and invoke
///   `render_markdown_to_pdf` from there — the bundled
///   `wasm/worker-client.js` provides a drop-in client.
///
/// [worker]: https://developer.mozilla.org/en-US/docs/Web/API/Worker
///
/// # Example (JavaScript)
///
/// ```js
/// import init, { render_markdown_to_pdf } from './pkg/pdfrs.js';
///
/// async function run() {
///     await init();
///     const pdfBytes = render_markdown_to_pdf("# Hello WASM\n\nIt works!");
///     // pdfBytes is a Uint8Array
/// }
/// ```
#[wasm_bindgen]
pub fn render_markdown_to_pdf(md: &str) -> Result<Vec<u8>, JsValue> {
    let elements = crate::elements::parse_markdown(md);
    let layout = crate::pdf_generator::PageLayout::portrait();

    crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Promise-returning variant of [`render_markdown_to_pdf`].
///
/// `wasm-bindgen` compiles this `async fn` to a JS function that
/// returns a `Promise<Uint8Array>`, so callers can `await` it without
/// explicitly setting up a Worker. Generation still runs on the WASM
/// thread that was initialised (the main thread for the standard
/// `import init, { ... } from './pkg/pdfrs.js'` flow). For documents
/// large enough to cause noticeable UI jank, prefer the Worker-based
/// `PdfWorkerClient` from `wasm/worker-client.js`, which moves both
/// the WASM module and the generation off the main thread.
///
/// # Example (JavaScript)
///
/// ```js
/// import init, { render_markdown_to_pdf_async } from './pkg/pdfrs.js';
///
/// await init();
/// const pdfBytes = await render_markdown_to_pdf_async("# Hello WASM");
/// ```
#[wasm_bindgen]
pub async fn render_markdown_to_pdf_async(md: String) -> Result<Vec<u8>, JsValue> {
    let elements = crate::elements::parse_markdown(&md);
    let layout = crate::pdf_generator::PageLayout::portrait();

    crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Return the version of the pdfrs crate (useful for cache invalidation).
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
