//! Clipping-path support: device-space coverage masks.
//!
//! A clipping path is rasterised once, when the path is consumed by a
//! path-painting operator, into a bounding-box-limited coverage mask. The
//! mask lives in device (screen) space, so it stays valid when the CTM
//! changes afterwards, and it can be intersected cheaply when a nested `q` /
//! `Q` scope adds another clip.

/// A device-space coverage mask.
#[derive(Debug, Clone)]
pub(super) struct ClipMask {
    /// Left edge of the mask in device pixels.
    x0: i32,
    /// Top edge of the mask in device pixels.
    y0: i32,
    /// Mask width in pixels.
    w: i32,
    /// Mask height in pixels.
    h: i32,
    /// Row-major coverage flags, `w * h` entries.
    bits: Vec<bool>,
}

impl ClipMask {
    /// A mask that covers nothing.
    pub(super) fn empty() -> Self {
        ClipMask {
            x0: 0,
            y0: 0,
            w: 0,
            h: 0,
            bits: Vec::new(),
        }
    }

    /// True when the pixel is inside the clip region.
    pub(super) fn contains(&self, x: i32, y: i32) -> bool {
        if self.w <= 0 || self.h <= 0 {
            return false;
        }
        let dx = x - self.x0;
        let dy = y - self.y0;
        if dx < 0 || dy < 0 || dx >= self.w || dy >= self.h {
            return false;
        }
        self.bits[(dy * self.w + dx) as usize]
    }

    /// Intersect two masks; the result covers only the pixels covered by both.
    pub(super) fn intersect(&self, other: &ClipMask) -> ClipMask {
        let x0 = self.x0.max(other.x0);
        let y0 = self.y0.max(other.y0);
        let x1 = (self.x0 + self.w).min(other.x0 + other.w);
        let y1 = (self.y0 + self.h).min(other.y0 + other.h);
        if x1 <= x0 || y1 <= y0 {
            return ClipMask::empty();
        }
        let w = x1 - x0;
        let h = y1 - y0;
        let mut bits = vec![false; (w * h) as usize];
        for y in y0..y1 {
            for x in x0..x1 {
                if self.contains(x, y) && other.contains(x, y) {
                    bits[((y - y0) * w + (x - x0)) as usize] = true;
                }
            }
        }
        ClipMask { x0, y0, w, h, bits }
    }

    /// Rasterise a set of device-space polygons into a coverage mask.
    ///
    /// `even_odd` selects the even-odd fill rule (PDF's `W*`); otherwise the
    /// non-zero winding rule is used (PDF's `W`).
    pub(super) fn rasterize(
        polys: &[Vec<(f32, f32)>],
        even_odd: bool,
        width: u32,
        height: u32,
    ) -> ClipMask {
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for poly in polys {
            for &(x, y) in poly {
                if !x.is_finite() || !y.is_finite() {
                    continue;
                }
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
        if min_x > max_x || min_y > max_y {
            return ClipMask::empty();
        }
        let x0 = (min_x.floor() as i32).max(0);
        let y0 = (min_y.floor() as i32).max(0);
        let x1 = (max_x.ceil() as i32 + 1).min(width as i32);
        let y1 = (max_y.ceil() as i32 + 1).min(height as i32);
        if x1 <= x0 || y1 <= y0 {
            return ClipMask::empty();
        }
        let w = x1 - x0;
        let h = y1 - y0;
        let mut bits = vec![false; (w * h) as usize];

        let mut crossings: Vec<(f32, i32)> = Vec::new();
        for y in y0..y1 {
            let yf = y as f32 + 0.5;
            crossings.clear();
            for poly in polys {
                let n = poly.len();
                if n < 2 {
                    continue;
                }
                for i in 0..n {
                    let (x1p, y1p) = poly[i];
                    let (x2p, y2p) = poly[(i + 1) % n];
                    if y1p == y2p {
                        continue;
                    }
                    // Half-open vertical interval [low, high) avoids double
                    // counting shared vertices.
                    let (lo, hi, dir) = if y1p < y2p {
                        (y1p, y2p, 1)
                    } else {
                        (y2p, y1p, -1)
                    };
                    if yf < lo || yf >= hi {
                        continue;
                    }
                    let t = (yf - y1p) / (y2p - y1p);
                    crossings.push((x1p + (x2p - x1p) * t, dir));
                }
            }
            if crossings.len() < 2 {
                continue;
            }
            crossings.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let mut winding = 0;
            for i in 0..crossings.len() - 1 {
                winding += crossings[i].1;
                let inside = if even_odd {
                    (i + 1) % 2 == 1
                } else {
                    winding != 0
                };
                if !inside {
                    continue;
                }
                let span_start = (crossings[i].0.ceil() as i32).max(x0);
                let span_end = (crossings[i + 1].0.floor() as i32).min(x1 - 1);
                if span_end < span_start {
                    continue;
                }
                for x in span_start..=span_end {
                    bits[((y - y0) * w + (x - x0)) as usize] = true;
                }
            }
        }

        ClipMask { x0, y0, w, h, bits }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterizes_a_rectangle() {
        let polys = vec![vec![(10.0, 10.0), (30.0, 10.0), (30.0, 20.0), (10.0, 20.0)]];
        let mask = ClipMask::rasterize(&polys, false, 100, 100);
        assert!(mask.contains(15, 15));
        assert!(mask.contains(29, 19));
        assert!(!mask.contains(9, 15));
        assert!(!mask.contains(15, 9));
        assert!(!mask.contains(31, 15));
    }

    #[test]
    fn nonzero_rule_fills_a_donut_but_even_odd_does_not() {
        // Outer square 10..50, inner square 20..40 (same winding direction).
        let outer = vec![(10.0, 10.0), (50.0, 10.0), (50.0, 50.0), (10.0, 50.0)];
        let inner = vec![(20.0, 20.0), (40.0, 20.0), (40.0, 40.0), (20.0, 40.0)];
        let nonzero = ClipMask::rasterize(&[outer.clone(), inner.clone()], false, 100, 100);
        assert!(nonzero.contains(30, 30), "non-zero rule fills the hole");
        let even_odd = ClipMask::rasterize(&[outer, inner], true, 100, 100);
        assert!(!even_odd.contains(30, 30), "even-odd rule leaves the hole");
    }

    #[test]
    fn intersect_keeps_only_shared_pixels() {
        let a = ClipMask::rasterize(
            &[vec![(0.0, 0.0), (50.0, 0.0), (50.0, 50.0), (0.0, 50.0)]],
            false,
            100,
            100,
        );
        let b = ClipMask::rasterize(
            &[vec![(25.0, 25.0), (75.0, 25.0), (75.0, 75.0), (25.0, 75.0)]],
            false,
            100,
            100,
        );
        let both = a.intersect(&b);
        assert!(both.contains(30, 30));
        assert!(!both.contains(10, 10));
        assert!(!both.contains(60, 60));
    }

    #[test]
    fn disjoint_intersection_covers_nothing() {
        let a = ClipMask::rasterize(
            &[vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]],
            false,
            100,
            100,
        );
        let b = ClipMask::rasterize(
            &[vec![(50.0, 50.0), (60.0, 50.0), (60.0, 60.0), (50.0, 60.0)]],
            false,
            100,
            100,
        );
        let both = a.intersect(&b);
        assert!(!both.contains(5, 5));
        assert!(!both.contains(55, 55));
    }
}
