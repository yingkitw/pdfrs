/**
 * IndexedDB caching for the pdfrs WASM binary.
 *
 * Caches the compiled .wasm file in IndexedDB so subsequent page loads
 * skip the network fetch entirely. Cache is keyed by the WASM version
 * (from the `version()` export) for automatic invalidation.
 */

const DB_NAME = 'pdfrs-wasm-cache';
const STORE_NAME = 'wasm-binaries';
const DB_VERSION = 1;

let dbPromise = null;

function openDB() {
  if (dbPromise) return dbPromise;

  dbPromise = new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);

    request.onupgradeneeded = (event) => {
      const db = event.target.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME);
      }
    };

    request.onsuccess = (event) => resolve(event.target.result);
    request.onerror = () => reject(request.error);
  });

  return dbPromise;
}

/**
 * Store a WASM binary in IndexedDB under the given key.
 *
 * @param {string} key - Cache key (typically the version string)
 * @param {ArrayBuffer} buffer - Raw WASM binary
 */
export async function cacheWasm(key, buffer) {
  try {
    const db = await openDB();
    const tx = db.transaction(STORE_NAME, 'readwrite');
    const store = tx.objectStore(STORE_NAME);
    store.put(buffer, key);
    return new Promise((resolve, reject) => {
      tx.oncomplete = resolve;
      tx.onerror = () => reject(tx.error);
    });
  } catch (e) {
    console.warn('[pdfrs] IndexedDB cache write failed:', e);
  }
}

/**
 * Retrieve a cached WASM binary from IndexedDB.
 *
 * @param {string} key - Cache key
 * @returns {Promise<ArrayBuffer|null>} - Cached binary or null if not found
 */
export async function getCachedWasm(key) {
  try {
    const db = await openDB();
    const tx = db.transaction(STORE_NAME, 'readonly');
    const store = tx.objectStore(STORE_NAME);
    const request = store.get(key);
    return new Promise((resolve, reject) => {
      request.onsuccess = () => resolve(request.result || null);
      request.onerror = () => reject(request.error);
    });
  } catch (e) {
    console.warn('[pdfrs] IndexedDB cache read failed:', e);
    return null;
  }
}

/**
 * Clear all cached WASM binaries.
 */
export async function clearCache() {
  try {
    const db = await openDB();
    const tx = db.transaction(STORE_NAME, 'readwrite');
    tx.objectStore(STORE_NAME).clear();
    return new Promise((resolve, reject) => {
      tx.oncomplete = resolve;
      tx.onerror = () => reject(tx.error);
    });
  } catch (e) {
    console.warn('[pdfrs] IndexedDB cache clear failed:', e);
  }
}

/**
 * Load the WASM module with IndexedDB caching.
 *
 * Fetches the .wasm binary, caches it in IndexedDB keyed by the crate
 * version, and returns the instantiated WASM module. On subsequent calls
 * with the same version, the cached binary is used directly.
 *
 * @param {string} wasmPath - Path to the .wasm file (e.g. 'pkg/pdfrs_bg.wasm')
 * @param {string} [versionKey] - Optional cache key (defaults to 'default')
 * @returns {Promise<WebAssembly.Module>} - Compiled WASM module
 */
export async function loadWasmWithCache(wasmPath, versionKey = 'default') {
  // Try cache first
  const cached = await getCachedWasm(versionKey);
  if (cached) {
    try {
      return await WebAssembly.compile(cached);
    } catch (e) {
      console.warn('[pdfrs] Cached WASM failed to compile, re-fetching:', e);
    }
  }

  // Fetch from network
  const response = await fetch(wasmPath);
  if (!response.ok) {
    throw new Error(`Failed to fetch WASM: ${response.status} ${response.statusText}`);
  }
  const buffer = await response.arrayBuffer();

  // Compile and cache
  const module = await WebAssembly.compile(buffer);
  await cacheWasm(versionKey, buffer);

  return module;
}
