//! Source: carrot_man.py geographic path extraction and relative-frame geometry.
pub type Point = (f64, f64);

pub fn haversine(a: Point, b: Point) -> f64 {
    let phi1 = a.1.to_radians();
    let phi2 = b.1.to_radians();
    let dphi = (b.1 - a.1).to_radians();
    let dlambda = (b.0 - a.0).to_radians();
    let value = (dphi / 2.).sin().powi(2) + phi1.cos() * phi2.cos() * (dlambda / 2.).sin().powi(2);
    2. * 6_371_000. * value.sqrt().atan2((1. - value).sqrt())
}

pub fn closest_point(a: Point, b: Point, position: Point) -> Point {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    if dx == 0. && dy == 0. {
        return a;
    }
    let t =
        (((position.0 - a.0) * dx + (position.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0., 1.);
    (a.0 + t * dx, a.1 + t * dy)
}

#[derive(Debug, serde::Serialize)]
pub struct ExtractedPath {
    pub points: Vec<Point>,
    pub start_index: i64,
    pub closest: Option<Point>,
}

pub fn path_after_distance(
    start_index: i64,
    coordinates: &[Point],
    position: Point,
    distance: f64,
) -> ExtractedPath {
    let start = usize::try_from(start_index.saturating_sub(2).max(0)).unwrap_or(usize::MAX);
    let mut closest = None;
    let mut min_distance = f64::INFINITY;
    for (i, segment) in coordinates.windows(2).enumerate().skip(start) {
        let candidate = closest_point(segment[0], segment[1], position);
        let value = haversine(position, candidate);
        if value < min_distance {
            min_distance = value;
            closest = Some((i, candidate));
        } else if value > min_distance && min_distance < 10. {
            break;
        }
    }
    let Some((index, closest)) = closest else {
        return ExtractedPath {
            points: Vec::new(),
            start_index: -1,
            closest: None,
        };
    };
    let mut points = vec![closest];
    let next = coordinates[index + 1];
    let mut total = haversine(closest, next);
    if total >= distance && total > 0. {
        points.push(interpolate(closest, next, distance / total));
    } else {
        points.push(next);
        for segment in coordinates[index + 1..].windows(2) {
            let length = haversine(segment[0], segment[1]);
            if total + length >= distance && length > 0. {
                points.push(interpolate(
                    segment[0],
                    segment[1],
                    (distance - total) / length,
                ));
                break;
            }
            total += length;
            points.push(segment[1]);
        }
    }
    ExtractedPath {
        points,
        start_index: i64::try_from(index).unwrap_or(i64::MAX),
        closest: Some(closest),
    }
}

pub fn interpolate(a: Point, b: Point, ratio: f64) -> Point {
    (a.0 + ratio * (b.0 - a.0), a.1 + ratio * (b.1 - a.1))
}

pub fn relative_xy(path: &[Point], reference: Point, heading: f64) -> Vec<Point> {
    let heading = heading.to_radians();
    path.iter()
        .map(|point| {
            let x = (point.0 - reference.0) * 40_008_000. * reference.1.to_radians().cos() / 360.;
            let y = (point.1 - reference.1) * 40_008_000. / 360.;
            (
                x * heading.sin() + y * heading.cos(),
                x * heading.cos() - y * heading.sin(),
            )
        })
        .collect()
}

pub fn curvature(a: Point, b: Point, c: Point) -> f64 {
    let v1 = (b.0 - a.0, b.1 - a.1);
    let v2 = (c.0 - b.0, c.1 - b.1);
    let len1 = (v1.0.powi(2) + v1.1.powi(2)).sqrt();
    let len2 = (v2.0.powi(2) + v2.1.powi(2)).sqrt();
    if len1 * len2 == 0. {
        0.
    } else {
        (v1.0 * v2.1 - v1.1 * v2.0) / (len1 * len2 * len1)
    }
}
