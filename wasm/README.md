# pdfrs WASM

WebAssembly build of **pdfrs** — generate PDFs from Markdown directly in the browser or Node.js.

## Build

Requires [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/):

```bash
wasm-pack build . --target web --out-dir wasm/pkg
```

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

run();
```

## API

### `render_markdown_to_pdf(md: string) => Uint8Array`

Converts a Markdown string to a PDF byte array.

## Features

- Pure in-memory processing (no filesystem access)
- Zero external runtime dependencies
- Small WASM binary (~500 KB with optimizations)
