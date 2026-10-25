//! Image loading, parsing, PDF embedding, and pixel filters
//!
//! Supports JPEG (baseline), PNG, and BMP formats. Images are parsed into
//! an [`ImageInfo`] struct containing raw pixel data and metadata, then
//! embedded into PDFs via the [`pdf_generator`](crate::pdf_generator) module.
//!
//! Pixel filters ([`ImageFilter`]) work on raw RGB (BMP, or PNG after scanline
//! reconstruction). JPEG must be converted to BMP/PNG first.
//!
//! The module is split into focused submodules:
//!
//! - `model` — the [`ImageInfo`], [`ImageFormat`] and [`ImageFilter`] types
//! - `load` — reading files and parsing PNG/BMP/JPEG containers
//! - `filters` — pixel filters and PNG scanline reconstruction
//! - `embed` — embedding images as PDF image XObjects

mod embed;
mod filters;
mod load;
mod model;

pub use embed::{
    add_image_to_pdf, create_bmp_image_object, create_filtered_image_pdf,
    create_image_content_stream, create_image_object, create_jpeg_image_object,
    create_png_image_object, scale_to_fit,
};
pub use filters::{apply_image_filter, apply_image_filters, ensure_raw_rgb};
pub use load::{load_image, load_image_with_alt_text};
pub use model::{ImageFilter, ImageFormat, ImageInfo, detect_image_format};

#[cfg(test)]
mod tests {
    use super::load::parse_jpeg_dimensions;
    use super::*;

    #[test]
    fn test_detect_jpeg() {
        let data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00];
        assert_eq!(detect_image_format(&data).unwrap(), ImageFormat::Jpeg);
    }

    #[test]
    fn test_detect_png() {
        let data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D];
        assert_eq!(detect_image_format(&data).unwrap(), ImageFormat::Png);
    }

    #[test]
    fn test_detect_bmp() {
        let data = vec![0x42, 0x4D, 0x00, 0x00];
        assert_eq!(detect_image_format(&data).unwrap(), ImageFormat::Bmp);
    }

    #[test]
    fn test_detect_unknown() {
        let data = vec![0x00, 0x00, 0x00, 0x00];
        assert!(detect_image_format(&data).is_err());
    }

    #[test]
    fn test_scale_to_fit() {
        // Image 800x600, max 400x400 -> scale by 0.5 -> 400x300
        let (w, h) = scale_to_fit(800, 600, 400.0, 400.0);
        assert!((w - 400.0).abs() < 0.01);
        assert!((h - 300.0).abs() < 0.01);
    }

    #[test]
    fn test_scale_no_upscale() {
        // Image 100x50, max 400x400 -> no upscale -> 100x50
        let (w, h) = scale_to_fit(100, 50, 400.0, 400.0);
        assert!((w - 100.0).abs() < 0.01);
        assert!((h - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_jpeg_dimensions() {
        // Minimal JPEG with SOF0 marker: FF D8 FF C0 00 11 08 <H:2> <W:2> ...
        let mut data = vec![0xFF, 0xD8]; // SOI
        // APP0 marker (skip)
        data.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00]);
        // SOF0 marker
        data.extend_from_slice(&[0xFF, 0xC0]);
        data.extend_from_slice(&[0x00, 0x11]); // length
        data.push(0x08); // precision
        data.extend_from_slice(&[0x01, 0x00]); // height = 256
        data.extend_from_slice(&[0x02, 0x00]); // width = 512
        data.extend_from_slice(&[0x03]); // components
        // pad
        data.extend_from_slice(&[0; 20]);

        let (w, h) = parse_jpeg_dimensions(&data).unwrap();
        assert_eq!(w, 512);
        assert_eq!(h, 256);
    }

    #[test]
    fn test_create_image_content_stream() {
        let cs = create_image_content_stream(100.0, 200.0, 300.0, 400.0, "Im1");
        let s = String::from_utf8(cs).unwrap();
        assert!(s.contains("q\n"));
        assert!(s.contains("300 0 0 400 100 200 cm"));
        assert!(s.contains("/Im1 Do"));
        assert!(s.contains("Q\n"));
    }

    fn sample_rgb_image() -> ImageInfo {
        ImageInfo {
            format: ImageFormat::Bmp,
            width: 2,
            height: 2,
            data: vec![
                255, 0, 0, // red
                0, 255, 0, // green
                0, 0, 255, // blue
                100, 100, 100, // gray
            ],
            bits_per_component: 8,
            color_components: 3,
            alt_text: None,
        }
    }

    #[test]
    fn test_image_filter_parse() {
        assert_eq!(
            ImageFilter::parse("grayscale").unwrap(),
            ImageFilter::Grayscale
        );
        assert_eq!(ImageFilter::parse("invert").unwrap(), ImageFilter::Invert);
        assert_eq!(ImageFilter::parse("sepia").unwrap(), ImageFilter::Sepia);
        assert_eq!(
            ImageFilter::parse("brightness:40").unwrap(),
            ImageFilter::Brightness(40)
        );
        assert_eq!(
            ImageFilter::parse("contrast:1.5").unwrap(),
            ImageFilter::Contrast(1.5)
        );
        assert!(ImageFilter::parse("blur").is_err());
    }

    #[test]
    fn test_grayscale_filter() {
        let filtered = sample_rgb_image()
            .apply_filter(ImageFilter::Grayscale)
            .unwrap();
        assert_eq!(filtered.data[0], filtered.data[1]);
        assert_eq!(filtered.data[1], filtered.data[2]);
        // Red luminance ≈ 0.299*255 ≈ 76
        assert!((70..=80).contains(&filtered.data[0]));
    }

    #[test]
    fn test_invert_filter() {
        let filtered = sample_rgb_image()
            .apply_filter(ImageFilter::Invert)
            .unwrap();
        assert_eq!(&filtered.data[0..3], &[0, 255, 255]); // inverted red
    }

    #[test]
    fn test_brightness_filter() {
        let filtered = sample_rgb_image()
            .apply_filter(ImageFilter::Brightness(50))
            .unwrap();
        assert_eq!(filtered.data[0], 255); // clamped
        assert_eq!(filtered.data[3], 50); // green 0 + 50
    }

    #[test]
    fn test_sepia_and_contrast_chain() {
        let filtered = sample_rgb_image()
            .apply_filters(&[ImageFilter::Sepia, ImageFilter::Contrast(1.2)])
            .unwrap();
        assert_eq!(filtered.data.len(), 12);
        assert_eq!(filtered.format, ImageFormat::Bmp);
    }

    #[test]
    fn test_jpeg_filter_rejected() {
        let jpeg = ImageInfo {
            format: ImageFormat::Jpeg,
            width: 1,
            height: 1,
            data: vec![0xFF, 0xD8, 0xFF],
            bits_per_component: 8,
            color_components: 3,
            alt_text: None,
        };
        assert!(jpeg.apply_filter(ImageFilter::Grayscale).is_err());
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn scale_preserves_aspect_ratio(width in 1u32..4000u32, height in 1u32..4000u32,
                                    max_w in 100f32..2000f32, max_h in 100f32..2000f32) {
            let (scaled_w, scaled_h) = scale_to_fit(width, height, max_w, max_h);

            // Check that scaled dimensions don't exceed max
            assert!(scaled_w <= max_w + 0.01, "Scaled width exceeds max");
            assert!(scaled_h <= max_h + 0.01, "Scaled height exceeds max");

            // Check that aspect ratio is preserved (within tolerance)
            let original_aspect = width as f32 / height as f32;
            let scaled_aspect = scaled_w / scaled_h;
            assert!((original_aspect - scaled_aspect).abs() < 0.01f32, "Aspect ratio not preserved");

            // Check that we don't upscale
            assert!(scaled_w <= width as f32 + 0.01, "Width was upscaled");
            assert!(scaled_h <= height as f32 + 0.01, "Height was upscaled");
        }
    }

    proptest! {
        #[test]
        fn scale_never_exceeds_bounds(width in 1u32..4000u32, height in 1u32..4000u32,
                                   max_w in 100f32..2000f32, max_h in 100f32..2000f32) {
            let (scaled_w, scaled_h) = scale_to_fit(width, height, max_w, max_h);
            // Allow small tolerance for floating point precision
            assert!(scaled_w <= max_w + 0.01, "Scaled width {} exceeds max_w {}", scaled_w, max_w);
            assert!(scaled_h <= max_h + 0.01, "Scaled height {} exceeds max_h {}", scaled_h, max_h);
        }
    }
}
