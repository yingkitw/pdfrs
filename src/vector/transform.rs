//! SVG `transform` attribute parsing and matrix composition.

vector_regex!(
    re_svg_transform,
    r"(?i)(translate|rotate|scale|matrix|skewx|skewy)\s*\(([^)]*)\)"
);

/// Parse `transform="translate(x,y) rotate(a) scale(s) matrix(a,b,c,d,e,f)"`.
pub fn parse_svg_transform(s: &str) -> [f32; 6] {
    let mut result = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let re = re_svg_transform();
    for caps in re.captures_iter(s) {
        let func = caps[1].to_ascii_lowercase();
        let args: Vec<f32> = caps[2]
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse::<f32>().ok())
            .collect();
        let next = match func.as_str() {
            "translate" => {
                let tx = args.first().copied().unwrap_or(0.0);
                let ty = args.get(1).copied().unwrap_or(0.0);
                [1.0, 0.0, 0.0, 1.0, tx, ty]
            }
            "scale" => {
                let sx = args.first().copied().unwrap_or(1.0);
                let sy = args.get(1).copied().unwrap_or(sx);
                [sx, 0.0, 0.0, sy, 0.0, 0.0]
            }
            "rotate" => {
                let a = args.first().copied().unwrap_or(0.0).to_radians();
                let cos = a.cos();
                let sin = a.sin();
                if args.len() >= 3 {
                    let cx = args[1];
                    let cy = args[2];
                    // T(cx,cy) * R(a) * T(-cx,-cy)
                    let m1 = [1.0, 0.0, 0.0, 1.0, cx, cy];
                    let m2 = [cos, sin, -sin, cos, 0.0, 0.0];
                    let m3 = [1.0, 0.0, 0.0, 1.0, -cx, -cy];
                    compose(&compose(&m1, &m2), &m3)
                } else {
                    [cos, sin, -sin, cos, 0.0, 0.0]
                }
            }
            "matrix" if args.len() == 6 => [args[0], args[1], args[2], args[3], args[4], args[5]],
            "skewx" => {
                let a = args.first().copied().unwrap_or(0.0).to_radians();
                [1.0, 0.0, a.tan(), 1.0, 0.0, 0.0]
            }
            "skewy" => {
                let a = args.first().copied().unwrap_or(0.0).to_radians();
                [1.0, a.tan(), 0.0, 1.0, 0.0, 0.0]
            }
            _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        };
        result = compose(&result, &next);
    }
    result
}

fn compose(a: &[f32; 6], b: &[f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svg_transform_translate() {
        let m = parse_svg_transform("translate(100,50)");
        assert_eq!(m, [1.0, 0.0, 0.0, 1.0, 100.0, 50.0]);
    }

    #[test]
    fn test_svg_transform_scale() {
        let m = parse_svg_transform("scale(2,3)");
        assert_eq!(m, [2.0, 0.0, 0.0, 3.0, 0.0, 0.0]);
        // Single-arg scale duplicates.
        let m = parse_svg_transform("scale(2)");
        assert_eq!(m, [2.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
    }

    #[test]
    fn test_svg_transform_rotate_around_origin() {
        let m = parse_svg_transform("rotate(90)");
        let cos90 = 0.0f32;
        let sin90 = 1.0f32;
        assert!((m[0] - cos90).abs() < 1e-5);
        assert!((m[1] - sin90).abs() < 1e-5);
        assert!((m[2] + sin90).abs() < 1e-5);
        assert!((m[3] - cos90).abs() < 1e-5);
    }

    #[test]
    fn test_svg_transform_matrix() {
        let m = parse_svg_transform("matrix(1,2,3,4,5,6)");
        assert_eq!(m, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }
}
