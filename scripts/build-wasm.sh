#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${1:-web}"
OUT_DIR="wasm/pkg"

echo "Building pdfrs for WASM target ($TARGET)..."
wasm-pack build . --target "$TARGET" --out-dir "$OUT_DIR" --features wasm --no-default-features

echo "WASM build complete: $OUT_DIR/"
