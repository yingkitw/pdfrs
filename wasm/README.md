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

Converts a Markdown string to a PDF byte array.

## Features

- Pure in-memory processing (no filesystem access)
- Zero external runtime dependencies (Rust side)
- Canvas-based live preview in browser demo
- Small WASM binary (~500 KB with optimizations)
