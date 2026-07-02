/**
 * Canvas-based PDF viewer for pdfrs WASM demo.
 * Renders PDF bytes (Uint8Array) onto an HTML canvas using pdf.js.
 */

const PDFJS_VERSION = '4.10.38';
const PDFJS_BASE = `https://cdn.jsdelivr.net/npm/pdfjs-dist@${PDFJS_VERSION}`;

let pdfjsLib = null;

async function loadPdfJs() {
  if (pdfjsLib) return pdfjsLib;

  pdfjsLib = await import(`${PDFJS_BASE}/build/pdf.mjs`);
  pdfjsLib.GlobalWorkerOptions.workerSrc = `${PDFJS_BASE}/build/pdf.worker.mjs`;
  return pdfjsLib;
}

/**
 * Render a PDF byte array onto a canvas element.
 *
 * @param {Uint8Array} pdfBytes - Raw PDF bytes from render_markdown_to_pdf()
 * @param {HTMLCanvasElement} canvas - Target canvas element
 * @param {object} [options]
 * @param {number} [options.page=1] - 1-based page number
 * @param {number} [options.scale=1.5] - Render scale factor
 * @returns {Promise<{ pageCount: number, pageNumber: number }>}
 */
export async function renderPdfToCanvas(pdfBytes, canvas, options = {}) {
  const { page = 1, scale = 1.5 } = options;
  const pdfjs = await loadPdfJs();

  const loadingTask = pdfjs.getDocument({ data: pdfBytes.slice() });
  const pdf = await loadingTask.promise;

  const pageNumber = Math.min(Math.max(1, page), pdf.numPages);
  const pdfPage = await pdf.getPage(pageNumber);
  const viewport = pdfPage.getViewport({ scale });

  const context = canvas.getContext('2d');
  canvas.width = viewport.width;
  canvas.height = viewport.height;

  await pdfPage.render({ canvasContext: context, viewport }).promise;

  return { pageCount: pdf.numPages, pageNumber };
}

/**
 * Render all pages of a PDF into a container, one canvas per page.
 *
 * @param {Uint8Array} pdfBytes - Raw PDF bytes
 * @param {HTMLElement} container - Element to append canvases into
 * @param {object} [options]
 * @param {number} [options.scale=1.5] - Render scale factor
 * @returns {Promise<{ pageCount: number }>}
 */
export async function renderPdfToContainer(pdfBytes, container, options = {}) {
  const { scale = 1.5 } = options;
  const pdfjs = await loadPdfJs();

  container.replaceChildren();

  const loadingTask = pdfjs.getDocument({ data: pdfBytes.slice() });
  const pdf = await loadingTask.promise;

  for (let i = 1; i <= pdf.numPages; i++) {
    const pdfPage = await pdf.getPage(i);
    const viewport = pdfPage.getViewport({ scale });

    const canvas = document.createElement('canvas');
    canvas.className = 'pdf-page';
    canvas.width = viewport.width;
    canvas.height = viewport.height;

    const context = canvas.getContext('2d');
    await pdfPage.render({ canvasContext: context, viewport }).promise;

    container.appendChild(canvas);
  }

  return { pageCount: pdf.numPages };
}
