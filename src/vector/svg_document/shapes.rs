//! SVG element → VectorCanvas rendering.

use super::context::{RenderCtx, SvgElement, SvgNode};
use super::style::{attr_f32, parse_paint, rounded_rect_path_ops};
use crate::pdf_generator::{Color, PageLayout};
use crate::vector::{PathOp, VectorShape, parse_svg_path, parse_svg_transform};

pub(super) fn render_element(node: &SvgNode, ctx: &mut RenderCtx) {
    let SvgNode::Element(el) = node else { return };
    match el.name.as_str() {
        "svg" => {
            // Root container: render children with inherited styles.
            with_style_scope(el, ctx, |ctx| render_children(el, ctx));
        }
        "defs" | "symbol" => {
            // Definitions are collected separately; don't render inline.
        }
        "g" => {
            let pushed = maybe_apply_transform(el, ctx);
            with_style_scope(el, ctx, |ctx| render_children(el, ctx));
            if pushed {
                ctx.pop_transform();
            }
        }
        "use" => render_use(el, ctx),
        "rect" => render_rect(el, ctx),
        "circle" => render_circle(el, ctx),
        "ellipse" => render_ellipse(el, ctx),
        "line" => render_line(el, ctx),
        "polyline" => render_polyline(el, ctx, false),
        "polygon" => render_polyline(el, ctx, true),
        "path" => render_path(el, ctx),
        "text" => render_text(el, ctx),
        _ => {
            // Unknown element: render children so nested shapes still appear.
            render_children(el, ctx);
        }
    }
}

pub(super) fn render_children(el: &SvgElement, ctx: &mut RenderCtx) {
    for child in &el.children {
        render_element(child, ctx);
    }
}

fn with_style_scope<F: FnOnce(&mut RenderCtx)>(el: &SvgElement, ctx: &mut RenderCtx, f: F) {
    let saved_fill = ctx.fill.clone();
    let saved_stroke = ctx.stroke.clone();
    let saved_sw = ctx.stroke_width;
    if let Some(s) = el.attrs.get("fill")
        && let parsed = parse_paint(s)
    {
        ctx.fill = parsed;
    }
    if let Some(s) = el.attrs.get("stroke")
        && let parsed = parse_paint(s)
    {
        ctx.stroke = parsed;
    }
    if let Some(s) = el.attrs.get("stroke-width")
        && let Some(w) = s.trim().trim_end_matches("px").parse::<f32>().ok()
    {
        ctx.stroke_width = w;
    }
    f(ctx);
    ctx.fill = saved_fill;
    ctx.stroke = saved_stroke;
    ctx.stroke_width = saved_sw;
}

pub(super) fn maybe_apply_transform(el: &SvgElement, ctx: &mut RenderCtx) -> bool {
    let Some(s) = el.attrs.get("transform") else {
        return false;
    };
    let m = parse_svg_transform(s);
    ctx.push_transform(m);
    true
}

/// Recursively collect elements with `id` attributes from `<defs>` and
/// `<symbol>` sections into the defs registry.
pub(super) fn collect_defs(el: &SvgElement, ctx: &mut RenderCtx) {
    for child in &el.children {
        let SvgNode::Element(child_el) = child else {
            continue;
        };
        if child_el.name == "defs" || child_el.name == "symbol" {
            collect_defs_children(child_el, ctx);
        } else if child_el.name == "g" {
            // Groups can also contain id'd elements.
            collect_defs(child_el, ctx);
        }
    }
}

pub(super) fn collect_defs_children(el: &SvgElement, ctx: &mut RenderCtx) {
    for child in &el.children {
        let SvgNode::Element(child_el) = child else {
            continue;
        };
        if let Some(id) = child_el.attrs.get("id") {
            ctx.defs.insert(id.clone(), child_el.clone());
        }
        // Recurse into nested groups.
        if child_el.name == "g" {
            collect_defs_children(child_el, ctx);
        }
    }
}

/// Render a `<use href="#id" x="..." y="...">` element by looking up the
/// referenced definition and rendering its children with a translate.
pub(super) fn render_use(el: &SvgElement, ctx: &mut RenderCtx) {
    let href = el
        .attrs
        .get("href")
        .or_else(|| el.attrs.get("xlink:href"))
        .map(|s| s.trim_start_matches('#').to_string());
    let Some(id) = href else { return };
    let Some(def_el) = ctx.defs.get(&id).cloned() else {
        return;
    };

    let x = attr_f32(el, "x", 0.0);
    let y = attr_f32(el, "y", 0.0);

    // Apply translate for the use position, then render the referenced element's children.
    ctx.push_transform([1.0, 0.0, 0.0, 1.0, x, y]);
    let pushed_style = false;
    with_style_scope(el, ctx, |ctx| {
        for child in &def_el.children {
            render_element(child, ctx);
        }
    });
    let _ = pushed_style;
    ctx.pop_transform();
}

pub(super) fn render_rect(el: &SvgElement, ctx: &mut RenderCtx) {
    let x = attr_f32(el, "x", 0.0);
    let y = attr_f32(el, "y", 0.0);
    let w = attr_f32(el, "width", 0.0);
    let h = attr_f32(el, "height", 0.0);
    let rx = attr_f32(el, "rx", 0.0);
    let ry = attr_f32(el, "ry", rx);
    let stroke = ctx.stroke_color();
    let fill = ctx.fill_color();
    let lw = ctx.line_width();
    let (x1, y1) = ctx.current().apply(x, y);
    let (x2, y2) = ctx.current().apply(x + w, y + h);
    let min_x = x1.min(x2);
    let min_y = y1.min(y2);
    let abs_w = (x2 - x1).abs();
    let abs_h = (y2 - y1).abs();
    if rx > 0.0 || ry > 0.0 {
        let r = rx.min(ry).min(abs_w / 2.0).min(abs_h / 2.0);
        let ops = rounded_rect_path_ops(min_x, min_y, abs_w, abs_h, r);
        ctx.canvas.push_shape(VectorShape::Path {
            ops,
            stroke,
            fill,
            line_width: lw,
        });
    } else {
        ctx.canvas.push_shape(VectorShape::Rect {
            x: min_x,
            y: min_y,
            width: abs_w,
            height: abs_h,
            stroke,
            fill,
            line_width: lw,
        });
    }
}

pub(super) fn render_circle(el: &SvgElement, ctx: &mut RenderCtx) {
    let cx = attr_f32(el, "cx", 0.0);
    let cy = attr_f32(el, "cy", 0.0);
    let r = attr_f32(el, "r", 0.0);
    let (x, y) = ctx.current().apply(cx, cy);
    ctx.canvas.push_shape(VectorShape::Ellipse {
        cx: x,
        cy: y,
        rx: r,
        ry: r,
        stroke: ctx.stroke_color(),
        fill: ctx.fill_color(),
        line_width: ctx.line_width(),
    });
}

pub(super) fn render_ellipse(el: &SvgElement, ctx: &mut RenderCtx) {
    let cx = attr_f32(el, "cx", 0.0);
    let cy = attr_f32(el, "cy", 0.0);
    let rx = attr_f32(el, "rx", 0.0);
    let ry = attr_f32(el, "ry", 0.0);
    let (x, y) = ctx.current().apply(cx, cy);
    ctx.canvas.push_shape(VectorShape::Ellipse {
        cx: x,
        cy: y,
        rx,
        ry,
        stroke: ctx.stroke_color(),
        fill: ctx.fill_color(),
        line_width: ctx.line_width(),
    });
}

pub(super) fn render_line(el: &SvgElement, ctx: &mut RenderCtx) {
    let x1 = attr_f32(el, "x1", 0.0);
    let y1 = attr_f32(el, "y1", 0.0);
    let x2 = attr_f32(el, "x2", 0.0);
    let y2 = attr_f32(el, "y2", 0.0);
    let (xa, ya) = ctx.current().apply(x1, y1);
    let (xb, yb) = ctx.current().apply(x2, y2);
    let stroke = ctx.stroke_color().unwrap_or(Color::black());
    ctx.canvas.push_shape(VectorShape::Line {
        x1: xa,
        y1: ya,
        x2: xb,
        y2: yb,
        stroke,
        width: ctx.line_width(),
    });
}

pub(super) fn render_polyline(el: &SvgElement, ctx: &mut RenderCtx, closed: bool) {
    let Some(s) = el.attrs.get("points") else {
        return;
    };
    let nums: Vec<f32> = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse::<f32>().ok())
        .collect();
    if nums.len() < 4 {
        return;
    }
    let mut pts: Vec<(f32, f32)> = nums
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| ctx.current().apply(c[0], c[1]))
        .collect();
    if closed
        && pts.first() != pts.last()
        && let Some(&(x0, y0)) = pts.first()
    {
        pts.push((x0, y0));
    }
    ctx.canvas.push_shape(VectorShape::Polygon {
        points: pts,
        stroke: ctx.stroke_color(),
        fill: ctx.fill_color(),
        line_width: ctx.line_width(),
    });
}

pub(super) fn render_path(el: &SvgElement, ctx: &mut RenderCtx) {
    let Some(d) = el.attrs.get("d").cloned() else {
        return;
    };
    let ops = match parse_svg_path(&d) {
        Ok(o) => o,
        Err(_) => return,
    };
    let ops: Vec<PathOp> = ops
        .iter()
        .map(|op| match op {
            PathOp::MoveTo { x, y } => {
                let (x, y) = ctx.current().apply(*x, *y);
                PathOp::MoveTo { x, y }
            }
            PathOp::LineTo { x, y } => {
                let (x, y) = ctx.current().apply(*x, *y);
                PathOp::LineTo { x, y }
            }
            PathOp::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => {
                let (x1, y1) = ctx.current().apply(*x1, *y1);
                let (x2, y2) = ctx.current().apply(*x2, *y2);
                let (x3, y3) = ctx.current().apply(*x3, *y3);
                PathOp::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                }
            }
            PathOp::Close => PathOp::Close,
        })
        .collect();
    ctx.canvas.push_shape(VectorShape::Path {
        ops,
        stroke: ctx.stroke_color(),
        fill: ctx.fill_color(),
        line_width: ctx.line_width(),
    });
}

pub(super) fn render_text(el: &SvgElement, ctx: &mut RenderCtx) {
    let x = attr_f32(el, "x", 0.0);
    let y = attr_f32(el, "y", 0.0);
    let (x, y) = ctx.current().apply(x, y);
    // Concatenate all descendant text.
    let mut text = String::new();
    collect_text(el, &mut text);
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    let size = el
        .attrs
        .get("font-size")
        .and_then(|s| {
            s.trim()
                .trim_end_matches("px")
                .trim_end_matches("pt")
                .parse::<f32>()
                .ok()
        })
        .unwrap_or(12.0);
    let fill = ctx.fill_color().unwrap_or(Color::black());
    ctx.canvas.push_shape(VectorShape::Text {
        x,
        y,
        text,
        size,
        fill,
    });
}

pub(super) fn collect_text(el: &SvgElement, out: &mut String) {
    for child in &el.children {
        match child {
            SvgNode::Text(s) => out.push_str(s),
            SvgNode::Element(e) if matches!(e.name.as_str(), "tspan" | "a") => {
                collect_text(e, out);
            }
            _ => {}
        }
    }
}

pub(super) fn apply_svg_viewbox(el: &SvgElement, ctx: &mut RenderCtx, layout: PageLayout) {
    let svg_w = attr_f32(el, "width", layout.width);
    let svg_h = attr_f32(el, "height", layout.height);
    if let Some(vb) = el.attrs.get("viewBox") {
        let nums: Vec<f32> = vb
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse::<f32>().ok())
            .collect();
        if nums.len() == 4 {
            let (vbx, vby, vbw, vbh) = (nums[0], nums[1], nums[2], nums[3]);
            if vbw > 0.0 && vbh > 0.0 {
                let sx = svg_w / vbw;
                let sy = svg_h / vbh;
                ctx.push_transform([sx, 0.0, 0.0, sy, -vbx * sx, -vby * sy]);
            }
        }
    }
}
