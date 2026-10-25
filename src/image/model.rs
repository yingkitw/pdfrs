//! The image model: [`ImageInfo`], [`ImageFormat`] and [`ImageFilter`].

use super::filters::{apply_image_filter, apply_image_filters};
use crate::error::{PdfError, Result};

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub bits_per_component: u8,
    pub color_components: u8, // 1=grayscale, 3=RGB, 4=RGBA
    /// Alternative text for accessibility (screen readers, alt text)
    pub alt_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Bmp,
}

/// Detect format from raw bytes
pub fn detect_image_format(data: &[u8]) -> Result<ImageFormat> {
    if data.len() < 4 {
        return Err(PdfError::Image("Image data too short".into()));
    }
    if data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        Ok(ImageFormat::Jpeg)
    } else if data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47 {
        Ok(ImageFormat::Png)
    } else if data[0] == 0x42 && data[1] == 0x4D {
        Ok(ImageFormat::Bmp)
    } else {
        Err(PdfError::Unsupported("Unsupported image format".into()))
    }
}

/// Load image from file, detect format, and extract dimensions and pixel data
impl ImageInfo {
    /// Set alternative text for accessibility
    pub fn with_alt_text(mut self, alt_text: String) -> Self {
        self.alt_text = Some(alt_text);
        self
    }

    /// Get the alternative text, or a default placeholder
    pub fn get_alt_text(&self) -> &str {
        self.alt_text.as_deref().unwrap_or("Image")
    }

    /// Apply a single pixel filter, converting to raw RGB when needed.
    pub fn apply_filter(self, filter: ImageFilter) -> Result<Self> {
        apply_image_filter(self, filter)
    }

    /// Apply multiple filters in order.
    pub fn apply_filters(self, filters: &[ImageFilter]) -> Result<Self> {
        apply_image_filters(self, filters)
    }
}

/// Pixel filters for raw RGB image data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFilter {
    /// Convert to luminance grayscale
    Grayscale,
    /// Invert each color channel
    Invert,
    /// Adjust brightness by `amount` (-255..=255)
    Brightness(i16),
    /// Adjust contrast by factor (1.0 = unchanged; typical range 0.5..=2.0)
    Contrast(f32),
    /// Apply a warm sepia tone
    Sepia,
}

impl ImageFilter {
    /// Parse a filter name from CLI text (`grayscale`, `invert`, `brightness:20`, `contrast:1.5`, `sepia`).
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim().to_ascii_lowercase();
        if s == "grayscale" || s == "grey" || s == "gray" {
            return Ok(Self::Grayscale);
        }
        if s == "invert" {
            return Ok(Self::Invert);
        }
        if s == "sepia" {
            return Ok(Self::Sepia);
        }
        if let Some(rest) = s.strip_prefix("brightness:") {
            let amount: i16 = rest
                .parse()
                .map_err(|_| PdfError::Image(format!("Invalid brightness amount: {rest}")))?;
            return Ok(Self::Brightness(amount.clamp(-255, 255)));
        }
        if let Some(rest) = s.strip_prefix("contrast:") {
            let factor: f32 = rest
                .parse()
                .map_err(|_| PdfError::Image(format!("Invalid contrast factor: {rest}")))?;
            return Ok(Self::Contrast(factor.max(0.0)));
        }
        Err(PdfError::Image(format!(
            "Unknown image filter '{s}' (expected grayscale, invert, sepia, brightness:N, contrast:F)"
        )))
    }
}
