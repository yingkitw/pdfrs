#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

PDF="comprehensive.pdf"
OUT="docs/assets"
CLI="${PDFCLI:-}"

if [[ -z "$CLI" ]]; then
  if [[ -x ./target/release/pdfcli ]]; then
    CLI=./target/release/pdfcli
  elif [[ -x ./target/debug/pdfcli ]]; then
    CLI=./target/debug/pdfcli
  else
    echo "Build pdfcli first (cargo build --release --bin pdfcli --features cli)" >&2
    exit 1
  fi
fi

mkdir -p "$OUT"
"$CLI" rasterize-pdf "$PDF" -o "$OUT/comprehensive-cover.png" --page 0 --dpi 120
"$CLI" rasterize-pdf "$PDF" -o "$OUT/comprehensive-interior.png" --page 18 --dpi 120
echo "Wrote $OUT/comprehensive-{cover,interior}.png"
