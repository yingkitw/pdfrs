//! PDF portfolio (collection) creation.

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::fs;

use crate::pdf::{PdfDocument, PdfObject, PdfValue};

/// Create a PDF portfolio (collection) that bundles multiple files.
///
/// A portfolio PDF wraps embedded files in a special `/Collection` catalog entry
/// that PDF readers can display as a navigable file grid or list.
///
/// # Arguments
///
/// * `output_file` — Path for the portfolio PDF output
/// * `files` — List of `(path, description)` tuples to embed
/// * `title` — Optional portfolio title
///
/// # Example
/// ```no_run
/// use pdfrs::pdf_ops::create_portfolio_pdf;
///
/// let files = vec![
///     ("report.pdf".to_string(), "Annual report".to_string()),
///     ("data.csv".to_string(), "Raw data".to_string()),
/// ];
/// create_portfolio_pdf("portfolio.pdf", &files, Some("Q3 Documents")).unwrap();
/// ```
pub fn create_portfolio_pdf(
    output_file: &str,
    files: &[(String, String)],
    title: Option<&str>,
) -> Result<()> {
    let mut doc = PdfDocument::new();

    // Create a catalog object so embed_file can update it
    let catalog_dict = HashMap::new();
    let catalog_id = 1;
    doc.objects
        .insert(catalog_id, PdfObject::Dictionary(catalog_dict));
    doc.catalog = catalog_id;

    // Embed each file
    let mut file_specs: Vec<(String, u32)> = Vec::new(); // (filename, file_spec_object_id)
    for (path, _desc) in files {
        let data = std::fs::read(path).map_err(|e| anyhow!("Cannot read {}: {}", path, e))?;
        let filename = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);
        let fs_id = doc.embed_file(filename, &data)?;
        file_specs.push((filename.to_string(), fs_id));
    }

    // Build the /Collection dictionary with a simple schema
    let mut collection_dict = HashMap::new();
    collection_dict.insert(
        "Type".to_string(),
        PdfValue::Object(PdfObject::String("/Collection".to_string())),
    );
    collection_dict.insert(
        "View".to_string(),
        PdfValue::Object(PdfObject::String("/D".to_string())),
    ); // Detailed list view

    // Schema — two columns: Filename and Description
    let mut schema_entries = Vec::new();
    schema_entries.push("/Name << /Type /F /O << /D [ (Name) ] >> >>".to_string());
    schema_entries.push("/Description << /Type /Desc /O << /D [ (Description) ] >> >>".to_string());
    let schema = format!("<< {} >>", schema_entries.join(" "));
    collection_dict.insert(
        "Schema".to_string(),
        PdfValue::Object(PdfObject::String(schema)),
    );

    // Sort entries for the portfolio
    let sort = "<< /S /Name /A true >>".to_string();
    collection_dict.insert(
        "Sort".to_string(),
        PdfValue::Object(PdfObject::String(sort)),
    );

    // Add collection object
    let next_id = doc.objects.keys().copied().max().unwrap_or(0) + 1;
    let collection_id = next_id;
    doc.objects
        .insert(collection_id, PdfObject::Dictionary(collection_dict));

    // Wire /Collection into catalog
    if let Some(PdfObject::Dictionary(catalog_dict)) = doc.objects.get_mut(&doc.catalog) {
        catalog_dict.insert(
            "Collection".to_string(),
            PdfValue::Object(PdfObject::String(format!("{} 0 R", collection_id))),
        );
        // Title if provided
        if let Some(t) = title {
            catalog_dict.insert(
                "Title".to_string(),
                PdfValue::Object(PdfObject::String(format!(
                    "({})",
                    super::escape_pdf_meta(t)
                ))),
            );
        }
    }

    fs::write(output_file, doc.to_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_portfolio_pdf() {
        let tmp_dir = std::env::temp_dir();
        let file1 = tmp_dir.join("portfolio_test1.txt");
        let file2 = tmp_dir.join("portfolio_test2.csv");
        let output = tmp_dir.join("test_portfolio.pdf");

        std::fs::write(&file1, b"Hello from file 1").unwrap();
        std::fs::write(&file2, b"a,b,c\n1,2,3").unwrap();

        let files = vec![
            (file1.to_string_lossy().to_string(), "Text file".to_string()),
            (file2.to_string_lossy().to_string(), "CSV data".to_string()),
        ];

        create_portfolio_pdf(&output.to_string_lossy(), &files, Some("Test Portfolio")).unwrap();

        assert!(output.exists(), "Portfolio PDF should be created");

        let bytes = std::fs::read(&output).unwrap();
        let content = String::from_utf8_lossy(&bytes);

        // Should contain Collection dictionary
        assert!(
            content.contains("/Collection"),
            "Should contain /Collection"
        );

        // Should contain embedded files
        assert!(
            content.contains("/EmbeddedFile"),
            "Should contain /EmbeddedFile"
        );
        assert!(content.contains("/Filespec"), "Should contain /Filespec");

        // Should contain the title
        assert!(
            content.contains("Test Portfolio"),
            "Should contain portfolio title"
        );

        // Should be a valid PDF
        assert!(content.starts_with("%PDF-"), "Should be a valid PDF");

        // Cleanup
        let _ = std::fs::remove_file(&file1);
        let _ = std::fs::remove_file(&file2);
        let _ = std::fs::remove_file(&output);
    }
}
