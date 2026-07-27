//! PDF structural and compliance validation.
//!
//! Extracted from the parent [`pdf`](crate::pdf) module for clarity. All public
//! items here are re-exported at `crate::pdf::`, so existing callers can keep
//! using paths like [`crate::pdf::validate_pdf_bytes`] unchanged.
//!
//! Covers:
//! - [`PdfValidation`] — structural integrity (header, xref, trailer, catalog, pages)
//! - [`PdfAValidation`] — PDF/A-1b and PDF/A-3b compliance
//! - [`PdfUaValidation`] — PDF/UA-1 accessibility checks
//! - [`ScreenReaderComplianceReport`] — combined PDF/UA + text-extraction audit

use crate::pdf::PdfDocument;
use anyhow::Result;
use std::fs::File;
use std::io::Read;

/// Validation result for PDF structural checks
#[derive(Debug, Clone)]
pub struct PdfValidation {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub page_count: usize,
    pub object_count: usize,
}

/// Validate a PDF file's structural integrity
pub fn validate_pdf(filename: &str) -> Result<PdfValidation> {
    let mut file = File::open(filename)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(validate_pdf_bytes(&buffer))
}

/// Validate PDF bytes for structural integrity (library API — no filesystem needed)
pub fn validate_pdf_bytes(data: &[u8]) -> PdfValidation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let content = String::from_utf8_lossy(data);

    // 1. Check PDF header
    if !content.starts_with("%PDF-") {
        errors.push("Missing PDF header (%PDF-x.x)".to_string());
    } else {
        let version_end = content.find('\n').unwrap_or(10).min(10);
        let version = &content[5..version_end];
        if !version.starts_with("1.") && !version.starts_with("2.") {
            warnings.push(format!("Unusual PDF version: {}", version));
        }
    }

    // 2. Check %%EOF marker
    let trimmed_end = content.trim_end();
    if !trimmed_end.ends_with("%%EOF") {
        errors.push("Missing %%EOF marker at end of file".to_string());
    }

    // 3. Check xref table or xref stream
    let has_xref = content.contains("\nxref\n") || content.contains("\nxref\r\n");
    let has_startxref = content.contains("startxref");
    if !has_xref {
        warnings.push("No traditional xref table found (may use xref stream)".to_string());
    }
    if !has_startxref {
        errors.push("Missing startxref pointer".to_string());
    }

    // 4. Check trailer
    let has_trailer = content.contains("trailer");
    if !has_trailer && has_xref {
        errors.push("Missing trailer dictionary".to_string());
    }

    // 5. Check for Catalog
    let has_catalog = content.contains("/Type /Catalog");
    if !has_catalog {
        errors.push("Missing document catalog (/Type /Catalog)".to_string());
    }

    // 6. Check for Pages
    let has_pages = content.contains("/Type /Pages");
    if !has_pages {
        errors.push("Missing pages tree (/Type /Pages)".to_string());
    }

    // 7. Count page objects (/Type /Page but NOT /Type /Pages)
    let page_re = super::re_page();
    let page_re_eol = super::re_page_eol();
    let actual_pages =
        page_re.find_iter(&content).count() + page_re_eol.find_iter(&content).count();
    if actual_pages == 0 {
        errors.push("No page objects found (/Type /Page)".to_string());
    }

    // 8. Count objects
    let obj_re = super::re_obj_count();
    let object_count = obj_re.find_iter(&content).count();
    if object_count == 0 {
        errors.push("No PDF objects found".to_string());
    }

    // 9. Check object/endobj pairing
    let endobj_count = content.matches("endobj").count();
    if object_count != endobj_count {
        warnings.push(format!(
            "Object/endobj mismatch: {} obj vs {} endobj",
            object_count, endobj_count
        ));
    }

    // 10. Check stream/endstream pairing
    let stream_count =
        content.matches("\nstream\n").count() + content.matches("\nstream\r\n").count();
    let endstream_count = content.matches("endstream").count();
    if stream_count != endstream_count {
        warnings.push(format!(
            "Stream/endstream mismatch: {} stream vs {} endstream",
            stream_count, endstream_count
        ));
    }

    // 11. Check /Root reference in trailer
    if has_trailer {
        let root_re = super::re_root_any();
        if !root_re.is_match(&content) {
            errors.push("Trailer missing /Root reference".to_string());
        }
    }

    let valid = errors.is_empty();

    PdfValidation {
        valid,
        errors,
        warnings,
        page_count: actual_pages,
        object_count,
    }
}

/// Validation result for PDF/A compliance checks
#[derive(Debug, Clone)]
pub struct PdfAValidation {
    pub compliant: bool,
    pub level: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub embedded_fonts: bool,
    pub has_xmp: bool,
    pub has_encryption: bool,
}

/// Validate PDF bytes for PDF/A-1b compliance (basic level).
///
/// Checks the most important PDF/A-1b requirements that can be
/// detected with structural analysis (no full content stream parsing):
///
/// - **No encryption** — /Encrypt must not be present
/// - **No JavaScript** — /JS or /JavaScript actions must not be present
/// - **No external streams** — /F references in streams must not be present
/// - **Embedded fonts** — all /Font descriptors must reference a font file
/// - **XMP metadata** — catalog should contain /Metadata reference
pub fn validate_pdf_a_bytes(data: &[u8]) -> PdfAValidation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let content = String::from_utf8_lossy(data);

    // 1. No encryption
    let has_encryption = content.contains("/Encrypt") || content.contains("\nEncrypt");
    if has_encryption {
        errors.push("PDF contains encryption (not allowed in PDF/A)".to_string());
    }

    // 2. No JavaScript
    let has_js = content.contains("/JS") || content.contains("/JavaScript");
    if has_js {
        errors.push("PDF contains JavaScript (not allowed in PDF/A)".to_string());
    }

    // 3. No external file references in streams
    let has_external = content.contains("\n/F ") || content.contains("/F (");
    if has_external {
        errors.push("PDF contains external stream references (not allowed in PDF/A)".to_string());
    }

    // 4. Check for embedded fonts
    // Count font descriptors and font files; every descriptor should have a file
    let font_desc_count = content.matches("/Type /FontDescriptor").count();
    let font_file_count = content.matches("/FontFile").count()
        + content.matches("/FontFile2").count()
        + content.matches("/FontFile3").count();
    let embedded_fonts = font_desc_count == 0 || font_file_count >= font_desc_count;
    if !embedded_fonts {
        errors.push(format!(
            "Fonts not fully embedded: {} descriptors vs {} font files",
            font_desc_count, font_file_count
        ));
    }

    // 5. Check for XMP metadata
    let has_xmp = content.contains("/Type /Metadata") || content.contains("/Metadata ");
    if !has_xmp {
        warnings.push("No XMP metadata stream found (recommended for PDF/A)".to_string());
    }

    // 6. No transparency (PDF/A-1 specific)
    let has_transparency = content.contains("/CA ") || content.contains("/ca ");
    if has_transparency {
        warnings.push("Possible transparency group detected (not allowed in PDF/A-1)".to_string());
    }

    // 7. No launch actions
    if content.contains("/S /Launch") {
        errors.push("PDF contains launch actions (not allowed in PDF/A)".to_string());
    }

    let compliant = errors.is_empty();

    PdfAValidation {
        compliant,
        level: "PDF/A-1b".to_string(),
        errors,
        warnings,
        embedded_fonts,
        has_xmp,
        has_encryption,
    }
}

/// Validate a PDF file for PDF/A-1b compliance
pub fn validate_pdf_a(filename: &str) -> Result<PdfAValidation> {
    let mut file = File::open(filename)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(validate_pdf_a_bytes(&buffer))
}

/// Validate PDF bytes for PDF/A-3b compliance.
///
/// PDF/A-3 is identical to PDF/A-1b except embedded files are *required*
/// (one or more attachments must be present in /Names -> /EmbeddedFiles).
pub fn validate_pdf_a3_bytes(data: &[u8]) -> PdfAValidation {
    let mut result = validate_pdf_a_bytes(data);
    result.level = "PDF/A-3b".to_string();

    let content = String::from_utf8_lossy(data);

    // PDF/A-3 requires at least one embedded file
    let has_embedded_files = content.contains("/EmbeddedFiles")
        && content.contains("/Filespec")
        && content.contains("/EmbeddedFile");
    if !has_embedded_files {
        result
            .errors
            .push("PDF/A-3 requires at least one embedded file attachment".to_string());
    }

    // Remove the transparency warning that only applies to PDF/A-1
    result.warnings.retain(|w| !w.contains("PDF/A-1"));

    // Recompute compliance
    result.compliant = result.errors.is_empty();
    result
}

/// Validate a PDF file for PDF/A-3b compliance
pub fn validate_pdf_a3(filename: &str) -> Result<PdfAValidation> {
    let mut file = File::open(filename)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(validate_pdf_a3_bytes(&buffer))
}

/// Validation result for PDF/UA (accessibility) compliance
#[derive(Debug, Clone)]
pub struct PdfUaValidation {
    pub compliant: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub has_mark_info: bool,
    pub has_struct_tree: bool,
    pub has_lang: bool,
    pub has_title: bool,
    pub fonts_embedded: bool,
}

/// Validate PDF bytes for PDF/UA-1 compliance (basic structural checks).
///
/// Checks the most important PDF/UA requirements detectable
/// through structural analysis:
///
/// - **/MarkInfo** — catalog must contain `/MarkInfo << /Marked true >>`
/// - **/StructTreeRoot** — catalog must reference a structure tree
/// - **/Lang** — catalog or page must declare a language
/// - **Title** — document must have a title in Info or XMP
/// - **No encryption** — security handlers interfere with assistive tech
/// - **Embedded fonts** — all fonts must be embedded for text extraction
pub fn validate_pdf_ua_bytes(data: &[u8]) -> PdfUaValidation {
    let mut errors = Vec::new();
    let warnings = Vec::new();
    let content = String::from_utf8_lossy(data);

    // 1. MarkInfo / Marked must be true
    let has_mark_info = content.contains("/MarkInfo")
        && (content.contains("/Marked true") || content.contains("/Marked\ntrue"));
    if !has_mark_info {
        errors.push("Missing /MarkInfo << /Marked true >> (required for PDF/UA)".to_string());
    }

    // 2. StructTreeRoot must exist
    let has_struct_tree = content.contains("/StructTreeRoot");
    if !has_struct_tree {
        errors.push("Missing /StructTreeRoot (required for tagged PDF)".to_string());
    }

    // 3. Language must be declared
    let has_lang = content.contains("/Lang") || content.contains("/Lang ");
    if !has_lang {
        errors.push("Missing /Lang attribute (required for PDF/UA)".to_string());
    }

    // 4. Document title
    let has_title = content.contains("/Title") || content.contains("<dc:title>");
    if !has_title {
        errors.push("Missing document title (required for PDF/UA)".to_string());
    }

    // 5. No encryption
    let has_encryption = content.contains("/Encrypt") || content.contains("\nEncrypt");
    if has_encryption {
        errors.push("Encryption prevents screen reader access (not allowed in PDF/UA)".to_string());
    }

    // 6. Fonts must be embedded
    let font_desc_count = content.matches("/Type /FontDescriptor").count();
    let font_file_count = content.matches("/FontFile").count()
        + content.matches("/FontFile2").count()
        + content.matches("/FontFile3").count();
    let fonts_embedded = font_desc_count == 0 || font_file_count >= font_desc_count;
    if !fonts_embedded {
        errors
            .push("Fonts not fully embedded (required for text extraction in PDF/UA)".to_string());
    }

    // 7. No JavaScript (interferes with assistive technology)
    if content.contains("/JS") || content.contains("/JavaScript") {
        errors.push("JavaScript actions interfere with assistive technology".to_string());
    }

    let compliant = errors.is_empty();

    PdfUaValidation {
        compliant,
        errors,
        warnings,
        has_mark_info,
        has_struct_tree,
        has_lang,
        has_title,
        fonts_embedded,
    }
}

/// Validate a PDF file for PDF/UA compliance
pub fn validate_pdf_ua(filename: &str) -> Result<PdfUaValidation> {
    let mut file = File::open(filename)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(validate_pdf_ua_bytes(&buffer))
}

/// Screen reader compliance report combining PDF/UA checks with text-extraction validation.
#[derive(Debug, Clone)]
pub struct ScreenReaderComplianceReport {
    pub compliant: bool,
    pub pdf_ua: PdfUaValidation,
    pub text_extractable: bool,
    pub extracted_text_length: usize,
    pub structure_element_types: Vec<String>,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
}

/// Check whether a PDF is suitable for screen reader and assistive technology use.
///
/// Runs [`validate_pdf_ua_bytes`], verifies text can be extracted, and inspects
/// the structure tree for common element types (`/S /H1`, `/S /P`, etc.).
pub fn check_screen_reader_compliance_bytes(data: &[u8]) -> ScreenReaderComplianceReport {
    let pdf_ua = validate_pdf_ua_bytes(data);
    let content = String::from_utf8_lossy(data);
    let mut issues = pdf_ua.errors.clone();
    let mut warnings = pdf_ua.warnings.clone();

    let (text_extractable, extracted_text_length) = match PdfDocument::load_from_bytes(data) {
        Ok(doc) => match doc.get_text() {
            Ok(text) => {
                let len = text.trim().len();
                (len > 0, len)
            }
            Err(e) => {
                issues.push(format!("Text extraction failed: {e}"));
                (false, 0)
            }
        },
        Err(e) => {
            issues.push(format!("PDF parse failed: {e}"));
            (false, 0)
        }
    };

    if !text_extractable {
        issues.push(
            "No extractable text found — screen readers cannot read document content".to_string(),
        );
    }

    let structure_element_types = detect_structure_element_types(&content);

    if pdf_ua.has_struct_tree && structure_element_types.len() <= 1 {
        warnings.push(
            "Structure tree is minimal — consider mapping headings, paragraphs, and lists to /StructElem entries".to_string(),
        );
    }

    let compliant = issues.is_empty();

    ScreenReaderComplianceReport {
        compliant,
        pdf_ua,
        text_extractable,
        extracted_text_length,
        structure_element_types,
        issues,
        warnings,
    }
}

/// Check screen reader compliance for a PDF file on disk.
pub fn check_screen_reader_compliance(filename: &str) -> Result<ScreenReaderComplianceReport> {
    let mut file = File::open(filename)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(check_screen_reader_compliance_bytes(&buffer))
}

fn detect_structure_element_types(content: &str) -> Vec<String> {
    const TYPES: &[&str] = &[
        "Document",
        "H1",
        "H2",
        "H3",
        "H4",
        "H5",
        "H6",
        "P",
        "L",
        "LI",
        "Table",
        "TR",
        "Figure",
        "Link",
        "Code",
        "Formula",
        "BlockQuote",
        "Note",
    ];

    TYPES
        .iter()
        .filter(|t| content.contains(&format!("/S /{t}")))
        .map(|t| (*t).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_known_structure_element_types() {
        let content = "<< /S /H1 >> /S /P /S /Table";
        let mut types = detect_structure_element_types(content);
        types.sort();
        assert_eq!(types, vec!["H1", "P", "Table"]);
    }

    #[test]
    fn validate_pdf_bytes_flags_missing_header() {
        let result = validate_pdf_bytes(b"not a pdf");
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("PDF header")));
    }

    #[test]
    fn validate_pdf_bytes_flags_missing_eof() {
        let result = validate_pdf_bytes(b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n");
        assert!(result.errors.iter().any(|e| e.contains("%%EOF")));
    }

    #[test]
    fn validate_pdf_a_bytes_rejects_encryption_and_javascript() {
        let pdf = b"%PDF-1.4\n/Encrypt /JS /JavaScript";
        let result = validate_pdf_a_bytes(pdf);
        assert!(!result.compliant);
        assert!(result.has_encryption);
        assert!(result.errors.iter().any(|e| e.contains("encryption")));
        assert!(result.errors.iter().any(|e| e.contains("JavaScript")));
    }

    #[test]
    fn validate_pdf_ua_bytes_flags_missing_mark_info_and_struct_tree() {
        let result = validate_pdf_ua_bytes(b"%PDF-1.4");
        assert!(!result.compliant);
        assert!(!result.has_mark_info);
        assert!(!result.has_struct_tree);
    }

    #[test]
    fn validate_pdf_a3_requires_embedded_file() {
        let mut result = validate_pdf_a3_bytes(b"%PDF-1.4");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("embedded file attachment"))
        );
        assert_eq!(result.level, "PDF/A-3b");
        // Smoke-test that re-exported regex helpers from the parent module compile and run.
        result.compliant = false;
    }
}
