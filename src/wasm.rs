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
//! # Usage (JavaScript)
//!
//! ```js
//! import init, { render_markdown_to_pdf } from './pkg/pdfrs.js';
//!
//! await init();
//! const pdfBytes = render_markdown_to_pdf("# Hello WASM\n\nIt works!");
//! // pdfBytes is a Uint8Array
//! ```
//!
//! # Web Worker offloading
//!
//! For large documents, use the provided worker to keep the UI responsive:
//!
//! ```js
//! import { PdfWorkerClient } from './worker-client.js';
//!
//! const client = new PdfWorkerClient();
//! await client.init();
//! const pdfBytes = await client.render('# Big document...');
//! ```
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

use wasm_bindgen::prelude::*;

/// Render Markdown to PDF bytes in a WebAssembly environment.
///
/// Takes a Markdown string and returns the raw PDF byte vector.
/// This function does **not** touch the filesystem — everything happens
/// in memory, making it ideal for browser or serverless WASM runtimes.
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

/// Return the version of the pdfrs crate (useful for cache invalidation).
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
