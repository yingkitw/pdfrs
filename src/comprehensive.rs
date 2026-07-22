//! Comprehensive document generation for capability demos and tests.
//!
//! Builds a multi-feature PDF from the bundled comprehensive Markdown fixture
//! (or any Markdown string) using callout plugins, subset fonts, and optional
//! linearization.

use crate::linearize;
use crate::optimization::{
    CompressionLevel, OptimizationProfile, OptimizationSettings, OptimizedPdfGenerator,
};
use crate::pdf_generator::PageLayout;
use crate::plugin::{parse_markdown_with_plugins, PluginRegistry};
use anyhow::Result;

/// Options for [`generate_comprehensive_pdf`].
#[derive(Debug, Clone)]
pub struct ComprehensiveOptions {
    pub font: String,
    pub font_size: f32,
    pub landscape: bool,
    pub linearize: bool,
    pub plugins_callouts: bool,
    pub columns: u8,
}

impl Default for ComprehensiveOptions {
    fn default() -> Self {
        Self {
            font: "Helvetica".into(),
            font_size: 11.0,
            landscape: false,
            // Default off: linearize rewrite can mangle outline string literals
            // until the PDF string round-trip is fully lossless. Opt in via CLI.
            linearize: false,
            plugins_callouts: true,
            columns: 1,
        }
    }
}

impl ComprehensiveOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_linearize(mut self, on: bool) -> Self {
        self.linearize = on;
        self
    }

    pub fn with_landscape(mut self, on: bool) -> Self {
        self.landscape = on;
        self
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_columns(mut self, columns: u8) -> Self {
        self.columns = columns.clamp(1, 4);
        self
    }
}

/// Bundled comprehensive Markdown fixture (same content as
/// `tests/fixtures/comprehensive_document.md`).
pub fn comprehensive_markdown() -> &'static str {
    include_str!("../tests/fixtures/comprehensive_document.md")
}

/// Generate a comprehensive PDF from Markdown bytes/string.
pub fn generate_comprehensive_pdf(markdown: &str, opts: &ComprehensiveOptions) -> Result<Vec<u8>> {
    let elements = if opts.plugins_callouts {
        let registry = PluginRegistry::with_defaults();
        parse_markdown_with_plugins(markdown, &registry)
    } else {
        crate::elements::parse_markdown(markdown)
    };

    let layout = if opts.landscape {
        PageLayout::landscape()
    } else {
        PageLayout::portrait()
    }
    .with_columns(opts.columns);

    let settings = OptimizationSettings::new()
        .with_compression(CompressionLevel::High)
        .with_subset_fonts(true)
        .with_linearize(false); // apply below for clearer control

    let mut bytes = OptimizedPdfGenerator::new(OptimizationProfile::custom(settings))
        .with_font(&opts.font)
        .with_font_size(opts.font_size)
        .with_layout(layout)
        .with_image_base_dir(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
        )
        .generate_bytes(&elements)?;

    if opts.linearize {
        bytes = linearize::linearize_pdf_bytes(&bytes)?;
    }
    Ok(bytes)
}

/// Generate the bundled comprehensive document.
pub fn generate_bundled_comprehensive_pdf(opts: &ComprehensiveOptions) -> Result<Vec<u8>> {
    generate_comprehensive_pdf(comprehensive_markdown(), opts)
}

/// Write the bundled comprehensive document to `output_path`.
pub fn write_bundled_comprehensive_pdf(output_path: &str, opts: &ComprehensiveOptions) -> Result<()> {
    let bytes = generate_bundled_comprehensive_pdf(opts)?;
    std::fs::write(output_path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::validate_pdf_bytes;

    #[test]
    fn test_bundled_comprehensive_generates_valid_pdf() {
        let opts = ComprehensiveOptions::default();
        let bytes = generate_bundled_comprehensive_pdf(&opts).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        let v = validate_pdf_bytes(&bytes);
        assert!(v.valid, "{:?}", v.errors);
        assert!(v.page_count >= 3, "expected multi-page, got {}", v.page_count);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/Outlines"));
        assert!(text.contains("Comprehensive Document") || text.contains("Part II"));
    }
}
