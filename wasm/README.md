# pdfrs WASM

WebAssembly build of **pdfrs** — generate PDFs from Markdown directly in the browser or Node.js.

## Build

Requires [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/):

```bash
./scripts/build-wasm.sh
# or
wasm-pack build . --target web --out-dir wasm/pkg --features wasm --no-default-features
```

## Browser Demo

Open `wasm/example.html` with a local HTTP server (required for ES modules):

```bash
./scripts/build-wasm.sh
python3 -m http.server 8080
# visit http://localhost:8080/wasm/example.html
```

The demo generates a PDF from Markdown via WASM and previews it on canvas using pdf.js.

## Usage (ES Modules)

```javascript
import init, { render_markdown_to_pdf } from './pkg/pdfrs.js';

async function run() {
  await init();

  const pdfBytes = render_markdown_to_pdf("# Hello WASM\n\nIt works!");
  // pdfBytes is a Uint8Array containing the raw PDF

  const blob = new Blob([pdfBytes], { type: 'application/pdf' });
  const url = URL.createObjectURL(blob);

  // Open or download
  const a = document.createElement('a');
  a.href = url;
  a.download = 'output.pdf';
  a.click();
}
```

## Canvas PDF Viewer

Use `viewer.js` to render generated PDF bytes onto canvas:

```javascript
import { renderPdfToCanvas, renderPdfToContainer } from './viewer.js';

// Single page on one canvas
await renderPdfToCanvas(pdfBytes, canvasElement, { page: 1, scale: 1.5 });

// All pages in a container (one canvas per page)
await renderPdfToContainer(pdfBytes, containerElement, { scale: 1.25 });
```

The viewer uses [pdf.js](https://mozilla.github.io/pdf.js/) from CDN for rendering.

## API

### `render_markdown_to_pdf(md: string) => Uint8Array`

Converts a Markdown string to a PDF byte array (synchronous).

### `render_markdown_to_pdf_async(md: string) => Promise<Uint8Array>`

Async version that yields to the JS event loop. Use in workers for
cooperative scheduling.

### `version() => string`

Returns the crate version (useful for cache invalidation).

## Web Worker Offloading

For large documents, offload PDF generation to a Web Worker to keep the
UI responsive:

```javascript
import { PdfWorkerClient } from './worker-client.js';

const client = new PdfWorkerClient('./worker.js');
await client.init('./pkg/pdfrs.js');

const pdfBytes = await client.render('# Big document...');
// pdfBytes is a Uint8Array (transferred from worker — zero copy)

client.terminate(); // when done
```

The worker loads the WASM module once and handles multiple `render()`
calls without re-initializing. PDF bytes are transferred using
`Transferable` for zero-copy delivery.

## IndexedDB Caching

Cache the compiled WASM binary in IndexedDB for instant page reloads:

```javascript
import { loadWasmWithCache, clearCache } from './cache.js';

// Load WASM with cache (keyed by version for auto-invalidation)
const wasmModule = await loadWasmWithCache('pkg/pdfrs_bg.wasm', '0.1.8');

// Clear cache when needed
await clearCache();
```

The cache is keyed by the crate version (from `version()`), so updating
the WASM binary automatically invalidates old entries.

## Features

- Pure in-memory processing (no filesystem access)
- Zero external runtime dependencies (Rust side)
- Canvas-based live preview in browser demo
- Small WASM binary (~500 KB with optimizations)
- Web Worker offloading for responsive UI
- IndexedDB caching for instant reloads
- Async API for cooperative scheduling in workers
