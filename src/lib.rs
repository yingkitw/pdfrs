//! # PDF-CLI Library
//!
//! A comprehensive Rust library for reading, writing, and manipulating PDF documents.
//! This library provides functionality for:
//!
//! - **PDF Generation**: Create PDFs from markdown or raw text content
//! - **PDF Parsing**: Extract text and structure from existing PDFs
//! - **PDF Manipulation**: Merge, split, rotate, and reorder pages
//! - **Image Support**: Embed JPEG, PNG, and BMP images in PDFs
//! - **Annotations**: Add text, link, and highlight annotations
//! - **Forms**: Create interactive PDF forms with text fields, checkboxes, radio buttons, and dropdowns
//! - **Watermarks**: Add text or image watermarks to PDFs
//! - **Metadata**: Manage document metadata including custom fields
//! - **Security**: Add password protection and permissions to PDFs
//!
//! ## Quick Start
//!
//! ```rust
//! use pdf_rs::pdf_generator;
//! use pdf_rs::elements;
//!
//! // Parse markdown content
//! let markdown = "# Hello World\n\nThis is a test document.";
//! let elements = elements::parse_markdown(markdown);
//!
//! // Generate PDF
//! let layout = pdf_generator::PageLayout::portrait();
//! pdf_generator::create_pdf_from_elements_with_layout(
//!     "output.pdf",
//!     &elements,
//!     "Helvetica",
//!     12.0,
//!     layout,
//! ).expect("Failed to create PDF");
//! ```
//!
//! ## Modules
//!
//! - [`pdf`]: PDF document parsing and text extraction
//! - [`pdf_generator`]: PDF generation from elements and content streams
//! - [`pdf_ops`]: High-level PDF operations (merge, split, watermark, etc.)
//! - [`elements`]: Markdown parsing and element representation
//! - [`markdown`]: Markdown to PDF conversion utilities
//! - [`image`]: Image loading, parsing, and PDF embedding
//! - [`compression`]: Data compression utilities
//! - [`security`]: PDF security, encryption, and permission management
//!
//! ## Examples
//!
//! ### Creating a PDF from Markdown
//!
//! ```rust,no_run
//! use pdf_rs::markdown;
//!
//! markdown::markdown_to_pdf_full(
//!     "input.md",
//!     "output.pdf",
//!     "Helvetica",
//!     12.0,
//!     pdf_rs::pdf_generator::PageOrientation::Portrait,
//! ).expect("Failed to convert");
//! ```
//!
//! ### Merging PDFs
//!
//! ```rust,no_run
//! use pdf_rs::pdf_ops;
//!
//! pdf_ops::merge_pdfs(
//!     &["file1.pdf", "file2.pdf"],
//!     "merged.pdf",
//! ).expect("Failed to merge");
//! ```
//!
//! ### Adding a Watermark
//!
//! ```rust,no_run
//! use pdf_rs::pdf_ops;
//!
//! pdf_ops::watermark_pdf(
//!     "input.pdf",
//!     "output.pdf",
//!     "CONFIDENTIAL",
//!     48.0,
//!     0.3,
//! ).expect("Failed to add watermark");
//! ```

pub mod compression;
pub mod elements;
pub mod image;
pub mod markdown;
pub mod pdf;
pub mod pdf_generator;
pub mod pdf_ops;
pub mod security;

#[cfg(test)]
mod tests {
    use crate::markdown::markdown_to_text;

    #[test]
    fn test_markdown_to_text() {
        let markdown = "# Header\n\nThis is **bold** and *italic* text.\n\n- Item 1\n- Item 2";
        let expected = "Header\nThis is bold and italic text.\n• Item 1\n• Item 2\n";
        let result = markdown_to_text(markdown);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_markdown_table_to_text() {
        let markdown = "| Name | Age |\n|------|-----|\n| John | 25  |\n| Jane | 30  |";
        let expected = "Name  Age  \n------  -----  \nJohn  25  \nJane  30  \n";
        let result = markdown_to_text(markdown);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_markdown_code_blocks() {
        let markdown = "Here is some code:\n\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\n\nMore text.";
        let expected =
            "Here is some code:\n\nfn main() {\n    println!(\"Hello\");\n}\n\nMore text.\n";
        let result = markdown_to_text(markdown);
        assert_eq!(result, expected);
    }
}
