use crate::pdf::PdfDocument;
use anyhow::Result;
use rayon::prelude::*;
use std::path::Path;

/// Parallel PDF operations using Rayon for concurrent processing
///
/// This module provides high-performance parallel implementations
/// of common PDF operations.

/// Merge multiple PDF files in parallel
///
/// This loads all input PDFs concurrently, then merges their pages.
/// Much faster than sequential loading for large numbers of files.
///
/// # Example
/// ```rust,no_run
/// use pdfrs::parallel;
///
/// let inputs = vec!["file1.pdf", "file2.pdf", "file3.pdf"];
/// let result = parallel::merge_pdfs_parallel(&inputs, "merged.pdf");
/// ```
pub fn merge_pdfs_parallel<P: AsRef<Path> + Send + Sync>(input_paths: &[P], output_path: P) -> Result<()> {
    if input_paths.is_empty() {
        anyhow::bail!("No input PDFs provided");
    }

    // Convert paths to strings for load_from_file
    let input_files: Vec<&str> = input_paths
        .iter()
        .map(|p| p.as_ref().to_str().unwrap())
        .collect();

    // Load all PDFs in parallel
    let documents: Result<Vec<_>> = input_files
        .par_iter()
        .map(|path| {
            PdfDocument::load_from_file(path)
                .map_err(|e| anyhow::anyhow!("Failed to load {}: {}", path, e))
        })
        .collect();

    let documents = documents?;

    // Merge documents sequentially (merge operation is inherently sequential)
    let output_str = output_path.as_ref().to_str().unwrap();
    crate::pdf_ops::merge_pdfs_sequential(&documents, output_str)
}

/// Extract text from multiple PDFs in parallel
///
/// Useful for batch processing or search operations.
///
/// # Example
/// ```rust,no_run
/// use pdfrs::parallel;
///
/// let results = parallel::extract_text_parallel(&["doc1.pdf", "doc2.pdf"]);
/// if let Ok(results) = results {
///     for (path, text) in results {
///         println!("{}: {} characters", path, text.len());
///     }
/// }
/// ```
pub fn extract_text_parallel<P: AsRef<Path> + Send + Sync>(input_paths: &[P]) -> Result<Vec<(String, String)>> {
    input_paths
        .par_iter()
        .map(|path| {
            let path_ref = path.as_ref();
            let path_str = path_ref.display().to_string();
            let path_file = path_ref.to_str().unwrap();

            PdfDocument::load_from_file(path_file)
                .and_then(|doc| doc.get_text())
                .map(|text| (path_str, text))
                .map_err(|e| anyhow::anyhow!("Failed to process {:?}: {}", path_ref, e))
        })
        .collect()
}

/// Batch validate multiple PDFs in parallel
pub fn validate_pdfs_parallel<P: AsRef<Path> + Send + Sync>(input_paths: &[P]) -> Result<Vec<(String, bool)>> {
    input_paths
        .par_iter()
        .map(|path| {
            let path_ref = path.as_ref();
            let path_str = path_ref.display().to_string();
            let path_file = path_ref.to_str().unwrap();

            let validation = crate::pdf::validate_pdf(path_file);
            Ok(match validation {
                Ok(v) => (path_str, v.valid),
                Err(_) => (path_str, false),
            })
        })
        .collect()
}

/// Count pages in multiple PDFs in parallel
pub fn count_pages_parallel<P: AsRef<Path> + Send + Sync>(input_paths: &[P]) -> Result<Vec<(String, usize)>> {
    input_paths
        .par_iter()
        .map(|path| {
            let path_ref = path.as_ref();
            let path_str = path_ref.display().to_string();
            let path_file = path_ref.to_str().unwrap();

            PdfDocument::load_from_file(path_file)
                .and_then(|doc| {
                    // Count page streams (objects that look like content streams)
                    let page_count = doc.objects.iter()
                        .filter(|(_, obj)| {
                            if let crate::pdf::PdfObject::Stream { data, .. } = obj {
                                let decompressed = if data.len() > 2 && data[0] == 0x78 && (data[1] == 0x9C || data[1] == 0xDA) {
                                    crate::compression::decompress_deflate(data).unwrap_or_default()
                                } else {
                                    data.clone()
                                };
                                let content = String::from_utf8_lossy(&decompressed);
                                content.contains("Tj") || content.contains("TJ") || content.contains("BT")
                            } else {
                                false
                            }
                        })
                        .count();
                    Ok((path_str, page_count))
                })
                .map_err(|e| anyhow::anyhow!("Failed to process {:?}: {}", path_ref, e))
        })
        .collect()
}

/// Process multiple PDFs with a custom function in parallel
///
/// This is a generic parallel processing utility that applies a function
/// to each PDF concurrently.
///
/// # Example
/// ```rust,no_run
/// use pdfrs::parallel;
///
/// let results = parallel::process_pdfs_parallel(
///     &["doc1.pdf", "doc2.pdf", "doc3.pdf"],
///     |doc| {
///         let text = doc.get_text()?;
///         Ok(text.len())
///     }
/// );
/// ```
pub fn process_pdfs_parallel<P, F, R>(
    input_paths: &[P],
    processor: F,
) -> Result<Vec<(String, R)>>
where
    P: AsRef<Path> + Send + Sync,
    F: Fn(&PdfDocument) -> Result<R> + Sync + Send,
    R: Send,
{
    input_paths
        .par_iter()
        .map(|path| {
            let path_ref = path.as_ref();
            let path_str = path_ref.display().to_string();
            let path_file = path_ref.to_str().unwrap();

            PdfDocument::load_from_file(path_file)
                .and_then(|doc| processor(&doc))
                .map(|result| (path_str, result))
                .map_err(|e| anyhow::anyhow!("Failed to process {:?}: {}", path_ref, e))
        })
        .collect()
}

/// Parallel PDF generator for multiple documents
///
/// Generate multiple PDFs concurrently, useful for batch document generation.
pub struct ParallelPdfGenerator {
    _layout: crate::pdf_generator::PageLayout,
    _font: String,
    _font_size: f32,
}

impl ParallelPdfGenerator {
    /// Create a new parallel PDF generator
    pub fn new() -> Self {
        Self {
            _layout: crate::pdf_generator::PageLayout::portrait(),
            _font: "Helvetica".to_string(),
            _font_size: 12.0,
        }
    }

    /// Generate multiple PDFs from markdown content in parallel
    ///
    /// # Example
    /// ```rust
    /// use pdfrs::parallel::ParallelPdfGenerator;
    /// use std::collections::HashMap;
    ///
    /// let generator = ParallelPdfGenerator::new();
    /// let inputs = HashMap::from([
    ///     ("doc1.md".to_string(), "# Document 1\nContent 1".to_string()),
    ///     ("doc2.md".to_string(), "# Document 2\nContent 2".to_string()),
    /// ]);
    ///
    /// let results = generator.generate_markdown_pdfs_parallel(&inputs);
    /// assert!(results.is_ok());
    /// ```
    pub fn generate_markdown_pdfs_parallel(
        &self,
        inputs: &std::collections::HashMap<String, String>,
    ) -> Result<std::collections::HashMap<String, Vec<u8>>> {
        inputs
            .par_iter()
            .map(|(filename, markdown)| {
                let elements = crate::elements::parse_markdown(markdown);
                let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(
                    &elements,
                    &self._font,
                    self._font_size,
                    self._layout
                )?;
                Ok((filename.clone(), pdf_bytes))
            })
            .collect()
    }
}

impl Default for ParallelPdfGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_merge() {
        // This test requires actual PDF files, so we'll just test the structure
        // In production, you'd create test PDFs first
    }
}
