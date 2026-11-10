//! Bounded cubic subdivision in world coordinates; tolerance is view-local.
const TOLERANCE_PIXELS: f64 = 0.75;
const MAX_DEPTH: u8 = 7; // At most 128 segments; recursion uses no heap storage.
type Point = [f64; 2];
type Cubic = [Point; 4];
fn distance_squared(point: Point, a: Point, b: Point) -> f64 {
    let delta = [b[0] - a[0], b[1] - a[1]];
    let length_squared = delta[0] * delta[0] + delta[1] * delta[1];
    let t = if length_squared == 0.0 {
        0.0
    } else {
        (((point[0] - a[0]) * delta[0] + (point[1] - a[1]) * delta[1]) / length_squared)
            .clamp(0.0, 1.0)
    };
    (point[0] - a[0] - t * delta[0]).powi(2) + (point[1] - a[1] - t * delta[1]).powi(2)
}
fn midpoint(a: Point, b: Point) -> Point {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}
fn split(points: Cubic) -> (Cubic, Cubic) {
    let a = midpoint(points[0], points[1]);
    let b = midpoint(points[1], points[2]);
    let c = midpoint(points[2], points[3]);
    let d = midpoint(a, b);
    let e = midpoint(b, c);
    let f = midpoint(d, e);
    ([points[0], a, d, f], [f, e, c, points[3]])
}
fn visit(
    points: Cubic,
    tolerance_squared: f64,
    depth: u8,
    emit: &mut impl FnMut([f32; 2], [f32; 2]),
) {
    // Distance to a finite segment (not its infinite line) also catches
    // collinear overshoot, backward connections and coincident endpoints.
    let flat = points[1..3]
        .iter()
        .all(|p| distance_squared(*p, points[0], points[3]) <= tolerance_squared);
    if flat || depth == MAX_DEPTH {
        emit(points[0].map(|v| v as f32), points[3].map(|v| v as f32));
    } else {
        let (left, right) = split(points);
        visit(left, tolerance_squared, depth + 1, emit);
        visit(right, tolerance_squared, depth + 1, emit);
    }
}
pub(crate) fn for_each_segment(
    points: [[f32; 2]; 4],
    zoom: f32,
    mut emit: impl FnMut([f32; 2], [f32; 2]),
) {
    // Scene validates zoom. f64 avoids cancellation during subdivision at large
    // world coordinates; GPU endpoints retain the existing f32 representation.
    let tolerance = TOLERANCE_PIXELS / f64::from(zoom);
    visit(
        points.map(|p| p.map(f64::from)),
        tolerance * tolerance,
        0,
        &mut emit,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn segments(points: [[f32; 2]; 4], zoom: f32) -> Vec<([f32; 2], [f32; 2])> {
        let mut result = Vec::new();
        for_each_segment(points, zoom, |a, b| result.push((a, b)));
        result
    }
    fn check(points: [[f32; 2]; 4], zoom: f32, expect_tolerance: bool) -> usize {
        let output = segments(points, zoom);
        assert!((1..=128).contains(&output.len()));
        assert_eq!(output.first().unwrap().0, points[0]);
        assert_eq!(output.last().unwrap().1, points[3]);
        for pair in output.windows(2) {
            assert_eq!(pair[0].1, pair[1].0);
        }
        assert!(
            output
                .iter()
                .flat_map(|(a, b)| a.iter().chain(b))
                .all(|v| v.is_finite())
        );
        if expect_tolerance {
            for i in 0..=4096 {
                let t = i as f64 / 4096.0;
                let u = 1.0 - t;
                let point = [0, 1].map(|axis| {
                    u.powi(3) * f64::from(points[0][axis])
                        + 3.0 * u * u * t * f64::from(points[1][axis])
                        + 3.0 * u * t * t * f64::from(points[2][axis])
                        + t.powi(3) * f64::from(points[3][axis])
                });
                let error = output
                    .iter()
                    .map(|(a, b)| distance_squared(point, a.map(f64::from), b.map(f64::from)))
                    .fold(f64::INFINITY, f64::min)
                    .sqrt()
                    * f64::from(zoom);
                assert!(
                    error <= TOLERANCE_PIXELS + 0.001,
                    "screen error {error} at t={t}"
                );
            }
        }
        output.len()
    }
    #[test]
    fn straight_and_degenerate_curves_use_one_segment() {
        for zoom in [0.02, 1.0, 16.0] {
            assert_eq!(
                check([[0., 0.], [50., 0.], [150., 0.], [200., 0.]], zoom, true),
                1
            );
            assert_eq!(check([[0., 0.]; 4], zoom, true), 1);
        }
    }
    #[test]
    fn bend_backward_and_coincident_endpoints_preserve_shape() {
        for points in [
            [[0., 0.], [100., 0.], [100., 120.], [200., 120.]],
            [[200., 0.], [300., 0.], [-100., 120.], [0., 120.]],
            [[0., 0.], [50., 0.], [-50., 0.], [0., 0.]],
            [[0., 0.], [100., 200.], [-100., 200.], [0., 0.]],
        ] {
            for zoom in [0.02, 1.0, 16.0] {
                check(points, zoom, true);
            }
        }
        assert!(check([[0., 0.], [50., 0.], [-50., 0.], [0., 0.]], 1.0, true) > 1);
    }
    #[test]
    fn zoom_refines_curves_and_extreme_coordinates_remain_bounded() {
        let points = [[0., 0.], [100., 0.], [100., 120.], [200., 120.]];
        let low = check(points, 0.02, true);
        let high = check(points, 16.0, true);
        assert!(low < high);
        check(
            [[-1e9, -1e9], [1e9, -1e9], [-1e9, 1e9], [1e9, 1e9]],
            16.0,
            false,
        );
        check(
            [
                [5e8, 5e8],
                [5e8 + 128., 5e8],
                [5e8 + 128., 5e8 + 128.],
                [5e8 + 256., 5e8 + 128.],
            ],
            16.0,
            false,
        );
    }
}
