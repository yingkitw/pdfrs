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
