//! Nearest clamped polyline normal for the unsupported-birth publication check.
use crate::scalar::norm;
/// Model y is right-positive; the returned normal uses radar coordinates.
pub fn birth_d_path(path: &[(f64, f64)], x: f64, y: f64) -> Result<Option<f64>, &'static str> {
    let points: Vec<_> = path.iter().map(|&(x, y)| (x, -y)).collect();
    let mut segments = vec![];
    for pair in points.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        let dx = x1 - x0;
        let dy = y1 - y0;
        let length = norm(&[dx, dy]);
        if length >= 1e-6 {
            segments.push((x0, y0, dx / length, dy / length, length));
        }
    }
    if segments.is_empty() {
        return points
            .first()
            .map(|p| Some(y - p.1))
            .ok_or("tuple index out of range");
    }
    let mut best_distance = f64::INFINITY;
    let mut best = None;
    for (x0, y0, tx, ty, length) in segments {
        let dx = tx * length;
        let dy = ty * length;
        let ratio = ((x - x0) * dx + (y - y0) * dy) / (length * length);
        // Python's ordered max(0, NaN) keeps its first argument.
        let lower = if ratio > 0. { ratio } else { 0. };
        let ratio = if lower < 1. { lower } else { 1. };
        let ox = x - (x0 + ratio * dx);
        let oy = y - (y0 + ratio * dy);
        let distance = ox * ox + oy * oy;
        if distance < best_distance {
            best_distance = distance;
            best = Some(-ty * ox + tx * oy);
        }
    }
    Ok(best)
}
