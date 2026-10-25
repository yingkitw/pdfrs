//! SVG render context, paint, and element tree types.

use super::TransformStackEntry;
use crate::pdf_generator::Color;
use crate::vector::VectorCanvas;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(super) enum SvgPaint {
    Inherit,
    Color(Color),
    None,
}

impl SvgPaint {
    pub(super) fn resolve(&self, fallback: Option<Color>) -> Option<Color> {
        match self {
            SvgPaint::Color(c) => Some(*c),
            SvgPaint::None => None,
            SvgPaint::Inherit => fallback,
        }
    }
}

pub(super) struct RenderCtx {
    pub(super) canvas: VectorCanvas,
    pub(super) transform_stack: Vec<TransformStackEntry>,
    pub(super) fill: SvgPaint,
    pub(super) stroke: SvgPaint,
    pub(super) stroke_width: f32,
    pub(super) defs: HashMap<String, SvgElement>,
}

impl RenderCtx {
    pub(super) fn current(&self) -> TransformStackEntry {
        *self
            .transform_stack
            .last()
            .unwrap_or(&TransformStackEntry::identity())
    }
    pub(super) fn push_transform(&mut self, m: [f32; 6]) {
        let parent = self.current();
        self.transform_stack.push(parent.concat(&m));
    }
    pub(super) fn pop_transform(&mut self) {
        if self.transform_stack.len() > 1 {
            self.transform_stack.pop();
        }
    }
    pub(super) fn fill_color(&self) -> Option<Color> {
        self.fill.resolve(Some(Color::black()))
    }
    pub(super) fn stroke_color(&self) -> Option<Color> {
        self.stroke.resolve(None)
    }
    pub(super) fn line_width(&self) -> f32 {
        if self.stroke_width.is_nan() {
            1.0
        } else {
            self.stroke_width
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum SvgNode {
    Element(SvgElement),
    Text(String),
}

#[derive(Debug, Clone)]
pub(crate) struct SvgElement {
    pub(crate) name: String,
    pub(crate) attrs: HashMap<String, String>,
    pub(crate) children: Vec<SvgNode>,
}
