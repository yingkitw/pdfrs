/**
 * Web Worker for pdfrs WASM PDF generation.
 *
 * Offloads PDF generation from the main thread to keep the UI responsive.
 * The worker loads the WASM module once and handles generation requests
 * via postMessage.
 *
 * @example
 * // Main thread:
 * const worker = new Worker('./worker.js', { type: 'module' });
 * worker.postMessage({ type: 'init', wasmPath: './pkg/pdfrs_bg.wasm' });
 * worker.onmessage = (e) => { ... };
 * worker.postMessage({ type: 'render', markdown: '# Hello' });
 */

let initialized = false;
let renderFn = null;
let versionFn = null;

self.onmessage = async (e) => {
  const { type, id } = e.data;

  switch (type) {
    case 'init': {
      try {
        const { wasmPath } = e.data;
        const jsPath = wasmPath.replace(/_bg\.wasm$/, '.js');

        // Dynamic import of the generated JS glue
        const mod = await import(jsPath);
        await mod.default();

        renderFn = mod.render_markdown_to_pdf;
        versionFn = mod.version || (() => 'unknown');
        initialized = true;

        self.postMessage({ type: 'ready', id, version: versionFn() });
      } catch (err) {
        self.postMessage({ type: 'error', id, error: err.message || String(err) });
      }
      break;
    }

    case 'render': {
      if (!initialized) {
        self.postMessage({ type: 'error', id, error: 'Worker not initialized' });
        return;
      }
      try {
        const { markdown } = e.data;
        const pdfBytes = renderFn(markdown);
        // Transfer the underlying ArrayBuffer for zero-copy
        self.postMessage(
          { type: 'result', id, pdfBytes },
          [pdfBytes.buffer]
        );
      } catch (err) {
        self.postMessage({ type: 'error', id, error: err.message || String(err) });
      }
      break;
    }

    case 'version': {
      if (versionFn) {
        self.postMessage({ type: 'version', id, version: versionFn() });
      } else {
        self.postMessage({ type: 'version', id, version: 'not-initialized' });
      }
      break;
    }

    default:
      self.postMessage({ type: 'error', id, error: `Unknown message type: ${type}` });
  }
};
