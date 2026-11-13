"""pdfrs — Python bindings for the pure-Rust PDF toolkit."""

from __future__ import annotations

from pdfrs._pdfrs import (
    html_to_pdf_bytes,
    merge_pdfs,
    render_markdown_to_pdf,
    validate_pdf_bytes,
    version,
)

__all__ = [
    "html_to_pdf_bytes",
    "merge_pdfs",
    "render_markdown_to_pdf",
    "validate_pdf_bytes",
    "version",
]

__version__ = version()
