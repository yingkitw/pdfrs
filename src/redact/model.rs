//! The redaction model: [`RedactionRegion`] and [`RedactionStyle`].

use crate::search::Rect;

pub struct RedactionRegion {
    /// Zero-indexed page number in document order.
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RedactionRegion {
    pub fn rect(&self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// How a redaction region is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionStyle {
    /// Replace intersecting text with spaces and overlay a black rectangle.
    BlackBox,
    /// Replace intersecting text with spaces only (no overlay).
    Strip,
}
