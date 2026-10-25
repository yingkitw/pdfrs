//! PDF high-level operations module
//!
//! This module provides high-level operations for manipulating PDF documents,
//! including merging, splitting, rotating, watermarking, and annotations.

mod annotations;
mod forms;
mod imaging;
mod manipulate;
mod metadata;
mod portfolio;
mod security;
#[cfg(feature = "sign")]
pub mod signing;
mod structure;
mod tables;

pub use annotations::*;
pub use forms::*;
pub use imaging::*;
pub use manipulate::*;
pub use metadata::*;
pub use portfolio::*;
pub use security::*;
pub use structure::*;
pub use tables::*;

use crate::error::{PdfError, Result};

/// Merge multiple PDF files into a single output PDF.
///
/// This function extracts page content from each input PDF and combines them
/// into a single output PDF, preserving the order of input files.
///
/// # Arguments
///
/// * `input_files` - Slice of file paths to merge
/// * `output_file` - Path where the merged PDF will be written
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if merging fails.
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::pdf_ops;
///
/// pdf_ops::merge_pdfs(
///     &["file1.pdf", "file2.pdf", "file3.pdf"],
///     "merged.pdf",
/// ).expect("Failed to merge PDFs");
/// ```
///
/// # Errors
///
/// This function will return an error if:
/// - No input files are provided
/// - Any input file cannot be read or parsed
pub(crate) fn escape_pdf_meta(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

pub(super) fn extract_pdf_dict_value(dict: &str, key: &str) -> Option<String> {
    // Search for key as a standalone token (followed by whitespace or end)
    let pos = dict
        .match_indices(key)
        .find(|(i, _)| {
            let end = i + key.len();
            end == dict.len()
                || dict[end..]
                    .starts_with(|c: char| c.is_whitespace() || c == '(' || c == '<' || c == '[')
        })
        .map(|(i, _)| i)?;
    let after = dict[pos + key.len()..].trim_start();
    if after.starts_with('(') {
        let end = after.find(')')?;
        Some(after[1..end].to_string())
    } else if after.starts_with('<') && !after.starts_with("<<") {
        let end = after.find('>')?;
        Some(after[1..end].to_string())
    } else if after.starts_with('[') {
        let end = after.find(']')?;
        Some(after[..=end].to_string())
    } else if let Some(name_after) = after.strip_prefix('/') {
        // PDF name: /Name
        let end = name_after
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>' || c == '[')
            .unwrap_or(name_after.len());
        Some(name_after[..end].to_string())
    } else {
        let end = after
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(after.len());
        Some(after[..end].to_string())
    }
}

/// Extract embedded images from a PDF and save them to an output directory.
///
/// Returns a list of saved file paths. Currently supports:
/// - JPEG images (`/Filter /DCTDecode`)
/// - Raw pixel streams saved as `.bin` for manual inspection
///
/// # Arguments
///
/// * `input_path` - Path to the input PDF file
/// * `output_dir` - Directory where extracted images will be saved
pub(super) fn extract_page_streams(doc: &crate::pdf::PdfDocument) -> Vec<Vec<u8>> {
    let mut streams = Vec::new();

    // Prefer structural extraction: traverse /Type /Page dictionaries and follow /Contents refs.
    let mut page_ids: Vec<u32> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let crate::pdf::PdfObject::Dictionary(dict) = obj
                && let Some(crate::pdf::PdfValue::Object(crate::pdf::PdfObject::String(kind))) =
                    dict.get("Type")
                && kind == "/Page"
            {
                return Some(*id);
            }
            None
        })
        .collect();
    page_ids.sort_unstable();

    for page_id in page_ids {
        if let Some(crate::pdf::PdfObject::Dictionary(dict)) = doc.objects.get(&page_id)
            && let Some(crate::pdf::PdfValue::Object(crate::pdf::PdfObject::String(
                contents_id_raw,
            ))) = dict.get("Contents")
            && let Ok(contents_id) = contents_id_raw.parse::<u32>()
            && let Some(crate::pdf::PdfObject::Stream { data, .. }) = doc.objects.get(&contents_id)
        {
            streams.push(decompress_if_needed(data));
        }
    }

    // Fallback for malformed/simple PDFs where /Page dictionaries are not parsed as expected.
    if !streams.is_empty() {
        return streams;
    }

    let mut sorted_ids: Vec<&u32> = doc.objects.keys().collect();
    sorted_ids.sort();
    for id in sorted_ids {
        if let crate::pdf::PdfObject::Stream { data, .. } = &doc.objects[id] {
            let decompressed = decompress_if_needed(data);
            let content = String::from_utf8_lossy(&decompressed);
            if content.contains("Tj") || content.contains("TJ") || content.contains("BT") {
                streams.push(decompressed);
            }
        }
    }

    streams
}

pub(super) fn decompress_if_needed(data: &[u8]) -> Vec<u8> {
    // Valid zlib header: CMF=0x78 and (CMF*256 + FLG) % 31 == 0
    if data.len() > 2
        && data[0] == 0x78
        && ((data[0] as u16) * 256 + (data[1] as u16)).is_multiple_of(31)
    {
        match crate::compression::decompress_deflate(data) {
            Ok(d) => d,
            Err(_) => data.to_vec(),
        }
    } else {
        data.to_vec()
    }
}

/// Build page content streams from elements via in-memory generation (no /tmp).
pub(super) fn build_page_streams(
    elements: &[crate::elements::Element],
    base_font_size: f32,
    _show_page_numbers: bool,
    layout: crate::pdf_generator::PageLayout,
    image_base_dir: Option<std::path::PathBuf>,
) -> Result<Vec<Vec<u8>>> {
    let bytes = crate::pdf_generator::generate_pdf_bytes_internal_with_base(
        elements,
        "Helvetica",
        base_font_size,
        layout,
        None,
        false,
        None,
        image_base_dir,
    )?;
    let doc = crate::pdf::PdfDocument::load_from_bytes(&bytes)?;
    let streams = extract_page_streams(&doc);
    if streams.is_empty() {
        return Err(PdfError::Parse(
            "No page content streams produced from elements".into(),
        ));
    }
    Ok(streams)
}

/// Assemble a merged PDF from raw page content streams
pub(super) fn assemble_merged_pdf(
    filename: &str,
    page_streams: &[Vec<u8>],
    font: &str,
    layout: &crate::pdf_generator::PageLayout,
) -> Result<()> {
    let metadata = metadata::PdfMetadata::default();
    metadata::assemble_pdf_with_metadata(filename, page_streams, font, layout, &metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_metadata_escape() {
        assert_eq!(escape_pdf_meta("hello (world)"), "hello \\(world\\)");
        assert_eq!(escape_pdf_meta("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn test_split_invalid_range() {
        let result = split_pdf("nonexistent.pdf", "out.pdf", 0, 5);
        assert!(result.is_err());
        let result = split_pdf("nonexistent.pdf", "out.pdf", 5, 3);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_empty_input() {
        let result = merge_pdfs(&[], "out.pdf");
        assert!(result.is_err());
    }

    #[test]
    fn test_rotate_invalid_angle() {
        let result = rotate_pdf("nonexistent.pdf", "out.pdf", 45);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid rotation"));
    }

    #[test]
    fn test_rotate_valid_angles() {
        // These will fail on file-not-found, not on validation
        for angle in [0, 90, 180, 270] {
            let result = rotate_pdf("nonexistent.pdf", "out.pdf", angle);
            assert!(result.is_err());
            assert!(!result.unwrap_err().to_string().contains("Invalid rotation"));
        }
    }

    #[test]
    fn test_create_pdf_with_images_empty() {
        let result = create_pdf_with_images("out.pdf", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No images"));
    }

    #[test]
    fn test_reorder_empty() {
        let result = reorder_pages("nonexistent.pdf", "out.pdf", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_build_watermark_stream() {
        let layout = crate::pdf_generator::PageLayout::portrait();
        let stream = build_watermark_stream("DRAFT", 48.0, 0.3, &layout);
        let content = String::from_utf8_lossy(&stream);
        assert!(content.contains("(DRAFT) Tj"));
        assert!(content.contains("0.7071")); // cos(45)
        assert!(content.contains("q\n")); // save state
        assert!(content.contains("Q\n")); // restore state
    }

    #[test]
    fn test_color_constructors() {
        let black = crate::pdf_generator::Color::black();
        assert_eq!(black.r, 0.0);
        assert_eq!(black.g, 0.0);
        assert_eq!(black.b, 0.0);

        let red = crate::pdf_generator::Color::red();
        assert_eq!(red.r, 1.0);

        let custom = crate::pdf_generator::Color::rgb(0.2, 0.4, 0.6);
        assert_eq!(custom.r, 0.2);
        assert_eq!(custom.g, 0.4);
        assert_eq!(custom.b, 0.6);
    }

    #[test]
    fn test_build_text_watermark_positions() {
        let layout = crate::pdf_generator::PageLayout::portrait();

        // Test different positions
        let center_stream =
            build_text_watermark_stream("TEST", 24.0, 0.5, &layout, WatermarkPosition::Center);
        assert!(String::from_utf8_lossy(&center_stream).contains("(TEST) Tj"));

        let diagonal_stream =
            build_text_watermark_stream("DRAFT", 48.0, 0.3, &layout, WatermarkPosition::Diagonal);
        let content = String::from_utf8_lossy(&diagonal_stream);
        assert!(content.contains("(DRAFT) Tj"));
        assert!(content.contains("0.707")); // cos(45°)
    }

    #[test]
    fn test_watermark_position_variants() {
        // Test that all watermark position variants work
        let layout = crate::pdf_generator::PageLayout::portrait();

        for position in [
            WatermarkPosition::Center,
            WatermarkPosition::TopLeft,
            WatermarkPosition::TopRight,
            WatermarkPosition::BottomLeft,
            WatermarkPosition::BottomRight,
            WatermarkPosition::Diagonal,
        ] {
            let stream = build_text_watermark_stream("TEST", 24.0, 0.5, &layout, position);
            assert!(!stream.is_empty());
        }
    }

    #[test]
    fn test_image_watermark_stream() {
        let layout = crate::pdf_generator::PageLayout::portrait();
        let image_info = crate::image::ImageInfo {
            format: crate::image::ImageFormat::Jpeg,
            width: 800,
            height: 600,
            data: vec![],
            bits_per_component: 8,
            color_components: 3,
            alt_text: None,
        };

        let result =
            build_image_watermark_stream(&image_info, 0.5, &layout, WatermarkPosition::Center);
        assert!(result.is_ok());

        let stream = result.unwrap();
        let content = String::from_utf8_lossy(&stream);
        assert!(content.contains("/Im1 Do"));
        assert!(content.contains("q\n"));
        assert!(content.contains("Q\n"));
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn escape_pdf_meta_roundtrip(s in ".*") {
            let escaped = escape_pdf_meta(&s);
            // After escaping, certain patterns should be consistent
            // Escaped parens should be present
            for c in s.chars() {
                match c {
                    '(' | ')' => {
                        // Should be escaped
                        assert!(escaped.contains(&format!(r"\{}", c)));
                    }
                    '\\' => {
                        // Should be escaped
                        assert!(escaped.contains(r"\\"));
                    }
                    _ => {}
                }
            }
        }
    }
}
