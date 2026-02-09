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
        // For now, we'll use the standard generation
        // In a full implementation, we would apply the optimization settings
        crate::pdf_generator::create_pdf_from_elements_with_layout(
            output_path,
            elements,
            &self.font,
            self.font_size,
            self.layout,
        )
    }

    /// Generate a PDF from elements and return the bytes
    pub fn generate_bytes(&self, elements: &[crate::elements::Element]) -> Result<Vec<u8>> {
        // For now, we'll use the standard generation
        // In a full implementation, we would apply the optimization settings
        crate::pdf_generator::generate_pdf_bytes(
            elements,
            &self.font,
            self.font_size,
            self.layout,
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
/// This function re-compresses PDF streams according to the optimization settings.
/// Note: This is a placeholder for a full implementation.
pub fn optimize_pdf_bytes(
    _pdf_data: &[u8],
    _settings: OptimizationSettings,
) -> Result<Vec<u8>> {
    // TODO: Implement full PDF optimization
    // This would involve:
    // - Parsing the PDF
    // - Recompressing streams with the specified compression level
    // - Downsampling images to the target DPI
    // - Subsetting fonts if requested
    // - Removing metadata if not preserving
    // - Linearizing the PDF if requested
    anyhow::bail!("PDF optimization not yet implemented")
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
}
