//! PDF optimization profiles for different use cases
//!
//! This module provides pre-configured optimization profiles for common scenarios:
//! - Web: Optimized for fast loading and small file size
//! - Print: High quality, larger file size
//! - Archive: Balanced compression and quality
//! - Ebook: Mobile-optimized with moderate compression

use crate::pdf_generator::PageLayout;
use anyhow::Result;

/// Optimization profile for PDF generation
///
/// Each profile defines trade-offs between file size, quality, and performance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationProfile {
    /// Web-optimized PDF (smallest file size, moderate quality)
    ///
    /// Best for: websites, email attachments, quick downloads
    /// - Higher compression
    /// - Downsampled images to 150 DPI
    /// - Subset fonts when possible
    /// - Remove metadata
    Web,

    /// Print-optimized PDF (highest quality, larger file size)
    ///
    /// Best for: professional printing, high-quality documents
    /// - Minimal compression
    /// - Images at 300 DPI or higher
    /// - Embed all fonts
    /// - Preserve all metadata
    Print,

    /// Archive-optimized PDF (balanced compression and quality)
    ///
    /// Best for: long-term storage, legal documents, records retention
    /// - Standard compression
    /// - Images at 200-300 DPI
    /// - Embed all fonts
    /// - Preserve all metadata
    Archive,

    /// Ebook-optimized PDF (mobile-friendly, moderate compression)
    ///
    /// Best for: e-readers, tablets, mobile devices
    /// - Moderate compression
    /// - Images at 150-200 DPI
    /// - Embed commonly used fonts
    /// - Tagged PDF for accessibility
    Ebook,

    /// Custom optimization profile with user-defined settings
    Custom(OptimizationSettings),
}

impl OptimizationProfile {
    /// Get the optimization settings for this profile
    pub fn settings(&self) -> OptimizationSettings {
        match self {
            OptimizationProfile::Web => OptimizationSettings {
                compression_level: CompressionLevel::High,
                image_dpi: 150,
                embed_fonts: false,
                subset_fonts: true,
                preserve_metadata: false,
                tagged_pdf: false,
                linearize: true, // Fast web view
            },
            OptimizationProfile::Print => OptimizationSettings {
                compression_level: CompressionLevel::Low,
                image_dpi: 300,
                embed_fonts: true,
                subset_fonts: false,
                preserve_metadata: true,
                tagged_pdf: false,
                linearize: false,
            },
            OptimizationProfile::Archive => OptimizationSettings {
                compression_level: CompressionLevel::Medium,
                image_dpi: 250,
                embed_fonts: true,
                subset_fonts: false,
                preserve_metadata: true,
                tagged_pdf: true,
                linearize: false,
            },
            OptimizationProfile::Ebook => OptimizationSettings {
                compression_level: CompressionLevel::Medium,
                image_dpi: 180,
                embed_fonts: true,
                subset_fonts: false,
                preserve_metadata: true,
                tagged_pdf: true,
                linearize: true,
            },
            OptimizationProfile::Custom(settings) => *settings,
        }
    }

    /// Web-optimized profile
    pub fn web() -> Self {
        OptimizationProfile::Web
    }

    /// Print-optimized profile
    pub fn print() -> Self {
        OptimizationProfile::Print
    }

    /// Archive-optimized profile
    pub fn archive() -> Self {
        OptimizationProfile::Archive
    }

    /// Ebook-optimized profile
    pub fn ebook() -> Self {
        OptimizationProfile::Ebook
    }

    /// Custom profile with specific settings
    pub fn custom(settings: OptimizationSettings) -> Self {
        OptimizationProfile::Custom(settings)
    }
}

impl Default for OptimizationProfile {
    fn default() -> Self {
        OptimizationProfile::Archive
    }
}

/// Detailed optimization settings for PDF generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptimizationSettings {
    /// Compression level for PDF streams
    pub compression_level: CompressionLevel,

    /// Target DPI for images (0 = no downsampling)
    pub image_dpi: u32,

    /// Whether to embed fonts in the PDF
    pub embed_fonts: bool,

    /// Whether to subset fonts (include only used characters)
    pub subset_fonts: bool,

    /// Whether to preserve document metadata
    pub preserve_metadata: bool,

    /// Whether to generate a tagged PDF (accessibility)
    pub tagged_pdf: bool,

    /// Whether to linearize the PDF (fast web view)
    pub linearize: bool,
}

impl Default for OptimizationSettings {
    fn default() -> Self {
        OptimizationProfile::Archive.settings()
    }
}

impl OptimizationSettings {
    /// Create a new OptimizationSettings with sensible defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the compression level
    pub fn with_compression(mut self, level: CompressionLevel) -> Self {
        self.compression_level = level;
        self
    }

    /// Set the target image DPI
    pub fn with_image_dpi(mut self, dpi: u32) -> Self {
        self.image_dpi = dpi;
        self
    }

    /// Set whether to embed fonts
    pub fn with_embed_fonts(mut self, embed: bool) -> Self {
        self.embed_fonts = embed;
        self
    }

    /// Set whether to subset fonts
    pub fn with_subset_fonts(mut self, subset: bool) -> Self {
        self.subset_fonts = subset;
        self
    }

    /// Set whether to preserve metadata
    pub fn with_preserve_metadata(mut self, preserve: bool) -> Self {
        self.preserve_metadata = preserve;
        self
    }

    /// Set whether to generate tagged PDF
    pub fn with_tagged_pdf(mut self, tagged: bool) -> Self {
        self.tagged_pdf = tagged;
        self
    }

    /// Set whether to linearize the PDF
    pub fn with_linearize(mut self, linearize: bool) -> Self {
        self.linearize = linearize;
        self
    }
}

/// Compression level for PDF content streams
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionLevel {
    /// No compression (fastest, largest files)
    None,
    /// Low compression (fast, moderately sized files)
    Low,
    /// Medium compression (balanced)
    Medium,
    /// High compression (slower, smallest files)
    High,
    /// Maximum compression (slowest, smallest files)
    Maximum,
}

impl CompressionLevel {
    /// Get the deflate compression level (0-9)
    pub fn deflate_level(&self) -> u8 {
        match self {
            CompressionLevel::None => 0,
            CompressionLevel::Low => 3,
            CompressionLevel::Medium => 6,
            CompressionLevel::High => 9,
            CompressionLevel::Maximum => 9,
        }
    }

    /// No compression
    pub fn none() -> Self {
        CompressionLevel::None
    }

    /// Low compression
    pub fn low() -> Self {
        CompressionLevel::Low
    }

    /// Medium compression
    pub fn medium() -> Self {
        CompressionLevel::Medium
    }

    /// High compression
    pub fn high() -> Self {
        CompressionLevel::High
    }

    /// Maximum compression
    pub fn maximum() -> Self {
        CompressionLevel::Maximum
    }
}

impl Default for CompressionLevel {
    fn default() -> Self {
        CompressionLevel::Medium
    }
}

/// Optimized PDF generator with profile-based settings
pub struct OptimizedPdfGenerator {
    profile: OptimizationProfile,
    settings: OptimizationSettings,
    layout: PageLayout,
    font: String,
    font_size: f32,
}

impl OptimizedPdfGenerator {
    /// Create a new optimized PDF generator with the specified profile
    pub fn new(profile: OptimizationProfile) -> Self {
        let settings = profile.settings();
        Self {
            profile,
            settings,
            layout: PageLayout::portrait(),
            font: "Helvetica".to_string(),
            font_size: 12.0,
        }
    }

    /// Set the page layout
    pub fn with_layout(mut self, layout: PageLayout) -> Self {
        self.layout = layout;
        self
    }

    /// Set the font
    pub fn with_font(mut self, font: &str) -> Self {
        self.font = font.to_string();
        self
    }

    /// Set the font size
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Generate a PDF from elements with the current optimization settings
    pub fn generate(&self, elements: &[crate::elements::Element], output_path: &str) -> Result<()> {
        let level = match self.settings.compression_level {
            CompressionLevel::None => None,
            _ => Some(self.settings.compression_level.deflate_level()),
        };
        crate::pdf_generator::create_pdf_from_elements_with_layout_and_compression(
            output_path,
            elements,
            &self.font,
            self.font_size,
            self.layout,
            level,
        )
    }

    /// Generate a PDF from elements and return the bytes
    pub fn generate_bytes(&self, elements: &[crate::elements::Element]) -> Result<Vec<u8>> {
        let level = match self.settings.compression_level {
            CompressionLevel::None => None,
            _ => Some(self.settings.compression_level.deflate_level()),
        };
        crate::pdf_generator::generate_pdf_bytes_with_compression(
            elements,
            &self.font,
            self.font_size,
            self.layout,
            level,
        )
    }

    /// Get the current optimization settings
    pub fn settings(&self) -> OptimizationSettings {
        self.settings
    }

    /// Get the current profile
    pub fn profile(&self) -> OptimizationProfile {
        self.profile
    }
}

impl Default for OptimizedPdfGenerator {
    fn default() -> Self {
        Self::new(OptimizationProfile::default())
    }
}

/// Apply optimization settings to existing PDF bytes
///
/// Re-compresses all PDF content streams using the specified compression level.
/// This reduces file size by re-encoding stream data with stronger (or weaker)
/// FlateDeflate compression as requested by the profile.
///
/// # Supported optimizations
///
/// - **Stream recompression**: Decompresses existing `/FlateDecode` streams and
///   recompresses them with the target deflate level.
/// - **Uncompressed stream compression**: Compresses streams that lack a `/Filter`
///   entry when the profile requests compression.
pub fn optimize_pdf_bytes(
    pdf_data: &[u8],
    settings: OptimizationSettings,
) -> Result<Vec<u8>> {
    use crate::pdf::{PdfDocument, PdfObject, PdfValue};
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut doc = PdfDocument::load_from_bytes(pdf_data)?;

    let target_level = settings.compression_level.deflate_level();

    for obj in doc.objects.values_mut() {
        if let PdfObject::Stream { dictionary, data } = obj {
            // Extract the filter name for content-aware decisions
            let filter = dictionary
                .get("Filter")
                .and_then(|v| match v {
                    PdfValue::Object(PdfObject::String(s)) => Some(s.as_str()),
                    _ => None,
                });

            // Check if this is an image XObject
            let is_image = dictionary
                .get("Subtype")
                .and_then(|v| match v {
                    PdfValue::Object(PdfObject::String(s)) => Some(s.as_str()),
                    _ => None,
                })
                .map(|s| s == "/Image" || s == "Image")
                .unwrap_or(false);

            // Content-aware image compression: skip already-compressed image formats
            // DCTDecode (JPEG), JPXDecode (JPEG2000), and JBIG2Decode are already
            // optimized with codecs designed for images; re-wrapping them in
            // FlateDecode usually increases file size and adds decode overhead.
            let is_already_compressed_image = is_image
                && filter.map(|f| {
                    f.contains("DCTDecode") || f.contains("JPXDecode") || f.contains("JBIG2Decode")
                }).unwrap_or(false);

            if is_already_compressed_image {
                // Preserve JPEG/JPEG2000/JBIG2 images as-is
                continue;
            }

            // Determine if the stream is currently FlateDecode-compressed
            let is_flate_compressed = filter
                .map(|f| f.contains("FlateDecode") || f.contains("flate"))
                .unwrap_or(false);

            // Get the raw uncompressed data
            let raw_data = if is_flate_compressed {
                crate::compression::decompress_deflate(data).unwrap_or_else(|_| data.clone())
            } else {
                data.clone()
            };

            // Recompress if target level is not None and we actually want compression
            if target_level > 0 {
                let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(target_level as u32));
                encoder.write_all(&raw_data)?;
                let compressed = encoder.finish()?;
                *data = compressed;
                dictionary.insert(
                    "Filter".to_string(),
                    PdfValue::Object(PdfObject::String("/FlateDecode".to_string())),
                );
            } else {
                *data = raw_data;
                dictionary.remove("Filter");
            }

            // Update Length to match the new data size
            dictionary.insert(
                "Length".to_string(),
                PdfValue::Object(PdfObject::Number(data.len() as f64)),
            );
        }
    }

    // Deduplicate identical objects (most effective after stream normalization)
    doc.deduplicate_objects();

    Ok(doc.to_bytes())
}

/// Apply an optimization profile to an existing PDF file
///
/// This is a convenience function that reads a PDF, applies the optimization
/// profile, and writes the result to a new file.
pub fn optimize_pdf_file(
    input_path: &str,
    output_path: &str,
    profile: OptimizationProfile,
) -> Result<()> {
    let data = std::fs::read(input_path)?;
    let optimized = optimize_pdf_bytes(&data, profile.settings())?;
    std::fs::write(output_path, optimized)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_settings() {
        let web_settings = OptimizationProfile::Web.settings();
        assert_eq!(web_settings.compression_level, CompressionLevel::High);
        assert_eq!(web_settings.image_dpi, 150);
        assert!(!web_settings.embed_fonts);
        assert!(web_settings.subset_fonts);
        assert!(web_settings.linearize);

        let print_settings = OptimizationProfile::Print.settings();
        assert_eq!(print_settings.compression_level, CompressionLevel::Low);
        assert_eq!(print_settings.image_dpi, 300);
        assert!(print_settings.embed_fonts);
        assert!(!print_settings.subset_fonts);
        assert!(!print_settings.linearize);
    }

    #[test]
    fn test_custom_settings() {
        let settings = OptimizationSettings::new()
            .with_compression(CompressionLevel::High)
            .with_image_dpi(200)
            .with_embed_fonts(true)
            .with_tagged_pdf(true);

        assert_eq!(settings.compression_level, CompressionLevel::High);
        assert_eq!(settings.image_dpi, 200);
        assert!(settings.embed_fonts);
        assert!(settings.tagged_pdf);
    }

    #[test]
    fn test_compression_level() {
        assert_eq!(CompressionLevel::None.deflate_level(), 0);
        assert_eq!(CompressionLevel::Low.deflate_level(), 3);
        assert_eq!(CompressionLevel::Medium.deflate_level(), 6);
        assert_eq!(CompressionLevel::High.deflate_level(), 9);
        assert_eq!(CompressionLevel::Maximum.deflate_level(), 9);
    }

    #[test]
    fn test_optimized_generator() {
        let generator = OptimizedPdfGenerator::new(OptimizationProfile::Web)
            .with_font("Courier")
            .with_font_size(10.0);

        assert_eq!(generator.profile(), OptimizationProfile::Web);
        assert_eq!(generator.settings().compression_level, CompressionLevel::High);
        assert_eq!(generator.font, "Courier");
        assert_eq!(generator.font_size, 10.0);
    }

    #[test]
    fn test_preserve_dctdecode_images() {
        use crate::pdf::{PdfDocument, PdfObject, PdfValue};
        use std::collections::HashMap;

        // Build a minimal PDF with a DCTDecode image XObject
        let mut doc = PdfDocument::new();

        // Catalog
        let mut catalog = HashMap::new();
        catalog.insert("Type".to_string(), PdfValue::Object(PdfObject::String("/Catalog".to_string())));
        doc.objects.insert(1, PdfObject::Dictionary(catalog));
        doc.catalog = 1;

        // DCTDecode image stream (minimal JPEG markers)
        let mut img_dict = HashMap::new();
        img_dict.insert("Type".to_string(), PdfValue::Object(PdfObject::String("/XObject".to_string())));
        img_dict.insert("Subtype".to_string(), PdfValue::Object(PdfObject::String("/Image".to_string())));
        img_dict.insert("Width".to_string(), PdfValue::Object(PdfObject::Number(1.0)));
        img_dict.insert("Height".to_string(), PdfValue::Object(PdfObject::Number(1.0)));
        img_dict.insert("ColorSpace".to_string(), PdfValue::Object(PdfObject::String("/DeviceRGB".to_string())));
        img_dict.insert("BitsPerComponent".to_string(), PdfValue::Object(PdfObject::Number(8.0)));
        img_dict.insert("Filter".to_string(), PdfValue::Object(PdfObject::String("/DCTDecode".to_string())));

        doc.objects.insert(5, PdfObject::Stream {
            dictionary: img_dict,
            data: b"\xFF\xD8\xFF\xD9".to_vec(), // Minimal JPEG SOI + EOI
        });

        // Also add an uncompressed text stream to ensure non-images still get compressed
        let mut text_dict = HashMap::new();
        text_dict.insert("Length".to_string(), PdfValue::Object(PdfObject::Number(12.0)));
        doc.objects.insert(10, PdfObject::Stream {
            dictionary: text_dict,
            data: b"BT /F1 12 Tf ET".to_vec(),
        });

        let pdf_bytes = doc.to_bytes();

        // Optimize with Web profile (high compression)
        let settings = OptimizationProfile::Web.settings();
        let optimized = optimize_pdf_bytes(&pdf_bytes, settings).unwrap();

        // Verify DCTDecode is preserved for the image
        let content = String::from_utf8_lossy(&optimized);
        assert!(content.contains("/DCTDecode"), "DCTDecode filter should be preserved for image streams");

        // Verify the text stream got compressed (should now have FlateDecode)
        assert!(content.contains("/FlateDecode"), "Non-image streams should be FlateDecode-compressed");
    }
}
