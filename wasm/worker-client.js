/**
 * Client-side wrapper for the pdfrs Web Worker.
 *
 * Provides a Promise-based API for offloading PDF generation to a
 * Web Worker, keeping the main thread responsive.
 *
 * @example
 * import { PdfWorkerClient } from './worker-client.js';
 *
 * const client = new PdfWorkerClient();
 * await client.init();
 * const pdfBytes = await client.render('# Hello World');
 * // Transferable Uint8Array — zero-copy from worker
 */

export class PdfWorkerClient {
  constructor(workerPath = './worker.js') {
    this.worker = new Worker(workerPath, { type: 'module' });
    this.nextId = 0;
    this.pending = new Map();

    this.worker.onmessage = (e) => {
      const { id, type } = e.data;
      const resolver = this.pending.get(id);
      if (!resolver) return;

      switch (type) {
        case 'ready':
        case 'version':
          this.pending.delete(id);
          resolver.resolve(e.data);
          break;
        case 'result':
          this.pending.delete(id);
          resolver.resolve(e.data.pdfBytes);
          break;
        case 'error':
          this.pending.delete(id);
          resolver.reject(new Error(e.data.error));
          break;
        default:
          this.pending.delete(id);
          resolver.reject(new Error(`Unexpected message type: ${type}`));
      }
    };

    this.worker.onerror = (e) => {
      // Reject all pending promises on worker error
      for (const [, resolver] of this.pending) {
        resolver.reject(new Error(e.message || 'Worker error'));
      }
      this.pending.clear();
    };
  }

  /**
   * Initialize the worker by loading the WASM module.
   *
   * @param {string} [wasmPath] - Path to the WASM JS glue file
   * @returns {Promise<string>} - WASM version string
   */
  init(wasmPath = './pkg/pdfrs.js') {
    return this._send('init', { wasmPath }).then((data) => data.version);
  }

  /**
   * Render Markdown to PDF bytes in the worker.
   *
   * @param {string} markdown - Markdown source
   * @returns {Promise<Uint8Array>} - PDF bytes (transferred from worker)
   */
  render(markdown) {
    return this._send('render', { markdown });
  }

  /**
   * Get the WASM version from the worker.
   *
   * @returns {Promise<string>}
   */
  version() {
    return this._send('version', {});
  }

  /**
   * Terminate the worker.
   */
  terminate() {
    this.worker.terminate();
    for (const [, resolver] of this.pending) {
      resolver.reject(new Error('Worker terminated'));
    }
    this.pending.clear();
  }

  _send(type, data) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ type, id, ...data });
    });
  }
}
