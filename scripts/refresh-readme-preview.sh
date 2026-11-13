#!/usr/bin/env bash
# Rasterize representative pages from comprehensive.pdf for the README gallery.
# Page numbers (0-based) were chosen with: pdfcli search-pdf comprehensive.pdf '<phrase>'
set -euo pipefail

cd "$(dirname "$0")/.."

PDF="comprehensive.pdf"
OUT="docs/assets"
CLI="${PDFCLI:-}"
DPI="${PREVIEW_DPI:-132}"

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

render() {
  local page="$1"
  local file="$2"
  "$CLI" rasterize-pdf "$PDF" -o "$OUT/$file" --page "$page" --dpi "$DPI"
}

render 0  readme-preview-cover.png
render 1  readme-preview-tables.png      # Scope matrix
render 4  readme-preview-lists.png       # Nested lists + task checklist
render 6  readme-preview-code.png        # Syntax-highlighted code blocks
render 7  readme-preview-math-callouts.png # Block math + NOTE callouts
render 11 readme-preview-columns.png    # Multi-column flow
render 12 readme-preview-images.png     # Embedded images / chart figures (optional)
render 21 readme-preview-charts.png     # Bar chart fence (Appendix K)
render 14 readme-preview-cjk.png        # Chinese / Japanese / Korean

# Legacy names (links may still exist elsewhere)
cp "$OUT/readme-preview-cover.png" "$OUT/comprehensive-cover.png"
cp "$OUT/readme-preview-charts.png" "$OUT/comprehensive-interior.png"

echo "Wrote $OUT/readme-preview-*.png (and legacy comprehensive-*.png aliases)"
