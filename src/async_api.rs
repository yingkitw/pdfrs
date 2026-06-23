//! Async PDF API for web servers and non-blocking I/O.
//!
//! This module provides `async` versions of key PDF operations.
//! It is gated behind the `async` Cargo feature and requires `tokio`.
//!
//! CPU-bound work (parsing, generation) is offloaded to `spawn_blocking`
//! so the runtime remains responsive.
//!
//! # Example
//! ```rust,no_run
//! use pdfrs::async_api::load_pdf_async;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let doc = load_pdf_async("input.pdf").await?;
//!     println!("Loaded {} objects", doc.objects.len());
//!     Ok(())
//! }
//! ```

use crate::pdf::{PdfDocument, PdfValidation, PdfAValidation};
use crate::optimization::OptimizationSettings;
use anyhow::Result;

/// Load a PDF file asynchronously.
///
/// Uses `tokio::fs::read` for non-blocking I/O, then parses the bytes
/// on a blocking thread pool so the runtime stays responsive.
pub async fn load_pdf_async(path: &str) -> Result<PdfDocument> {
    let bytes = tokio::fs::read(path).await?;
    let path = path.to_string();
    tokio::task::spawn_blocking(move || PdfDocument::load_from_bytes(&bytes))
        .await
        .map_err(|e| anyhow::anyhow!("Task panicked: {:?}", e))?
}

/// Generate a PDF from markdown text asynchronously.
///
/// Offloads the generation work to a blocking thread.
pub async fn generate_pdf_async(
    markdown: &str,
    font: &str,
    font_size: f32,
) -> Result<Vec<u8>> {
    let markdown = markdown.to_string();
    let font = font.to_string();
    tokio::task::spawn_blocking(move || {
        let elements = crate::elements::parse_markdown(&markdown);
        let layout = crate::pdf_generator::PageLayout::portrait();
        crate::pdf_generator::generate_pdf_bytes(&elements, &font, font_size, layout)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task panicked: {:?}", e))?
}

/// Optimize a PDF asynchronously.
///
/// Reads the file, optimizes on a blocking thread, and returns the
/// optimized bytes.
pub async fn optimize_pdf_async(
    path: &str,
    settings: OptimizationSettings,
) -> Result<Vec<u8>> {
    let bytes = tokio::fs::read(path).await?;
    tokio::task::spawn_blocking(move || {
        crate::optimization::optimize_pdf_bytes(&bytes, settings)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task panicked: {:?}", e))?
}

/// Validate a PDF file asynchronously.
///
/// Reads the file and runs structural validation on a blocking thread.
pub async fn validate_pdf_async(path: &str) -> Result<PdfValidation> {
    let bytes = tokio::fs::read(path).await?;
    tokio::task::spawn_blocking(move || {
        Ok(crate::pdf::validate_pdf_bytes(&bytes))
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task panicked: {:?}", e))?
}

/// Validate PDF/A-1b compliance asynchronously.
pub async fn validate_pdf_a_async(path: &str) -> Result<PdfAValidation> {
    let bytes = tokio::fs::read(path).await?;
    tokio::task::spawn_blocking(move || {
        Ok(crate::pdf::validate_pdf_a_bytes(&bytes))
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task panicked: {:?}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_pdf_async_roundtrip() {
        let elements = vec![
            crate::elements::Element::Paragraph { text: "Async test".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let tmp_path = std::env::temp_dir().join("async_test.pdf");
        tokio::fs::write(&tmp_path, &pdf_bytes).await.unwrap();

        let doc = load_pdf_async(&tmp_path.to_string_lossy()).await.unwrap();
        let text = doc.get_text().unwrap();
        assert!(text.contains("Async test"), "Should extract text from async-loaded PDF: {}", text);

        let _ = tokio::fs::remove_file(&tmp_path).await;
    }

    #[tokio::test]
    async fn test_generate_pdf_async() {
        let bytes = generate_pdf_async("# Hello\n\nWorld.", "Helvetica", 12.0).await.unwrap();
        assert!(!bytes.is_empty(), "Should generate non-empty PDF bytes");

        let content = String::from_utf8_lossy(&bytes);
        assert!(content.starts_with("%PDF-"), "Should be a valid PDF");
    }

    #[tokio::test]
    async fn test_validate_pdf_async() {
        let elements = vec![
            crate::elements::Element::Paragraph { text: "Validation test".into() },
        ];
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();

        let tmp_path = std::env::temp_dir().join("async_validate.pdf");
        tokio::fs::write(&tmp_path, &pdf_bytes).await.unwrap();

        let result = validate_pdf_async(&tmp_path.to_string_lossy()).await.unwrap();
        assert!(result.valid, "Generated PDF should be structurally valid: {:?}", result.errors);

        let _ = tokio::fs::remove_file(&tmp_path).await;
    }
}
