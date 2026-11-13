#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "Building Python wheels (maturin)..."
maturin build --release --out dist

echo "Wheels written to dist/"
echo "To publish: maturin publish --skip-existing"
