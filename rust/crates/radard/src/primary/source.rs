use super::{constants::*, VisionMatch};
use crate::point::Point;

pub fn primary_points(points: &[Point], mode: i32) -> Vec<Point> {
    let front: Vec<_> = points
        .iter()
        .filter(|point| point.source == "frontRadar" && point.d_rel > 0.2)
        .cloned()
        .collect();
    let scc: Vec<_> = points
        .iter()
        .filter(|point| point.source == "scc" && point.d_rel > 0.2)
        .cloned()
        .collect();
    if mode <= -2 {
        Vec::new()
    } else if mode <= 0 {
        scc
    } else if mode >= 3 {
        front.into_iter().chain(scc).collect()
    } else if mode == 2 {
        front
            .into_iter()
            .chain(
                scc.into_iter()
                    .filter(|point| point.v_lead < LOW_SPEED_SCC_MAX_VLEAD_MPS),
            )
            .collect()
    } else {
        front
    }
}

pub fn dpath_primary(points: &[Point], mode: i32) -> Vec<Point> {
    if mode == 3 {
        points
            .iter()
            .filter(|point| point.source == "frontRadar" && point.d_rel > 0.2)
            .cloned()
            .collect()
    } else {
        primary_points(points, mode)
    }
}

pub fn dpath_fallback(points: &[Point], mode: i32) -> Vec<Point> {
    let configured = primary_points(points, mode);
    if mode <= 0 {
        return configured;
    }
    let scc: std::collections::HashSet<_> = configured
        .iter()
        .filter(|point| point.source == "scc")
        .map(Point::identity)
        .collect();
    points
        .iter()
        .filter(|point| point.source != "scc" || scc.contains(&point.identity()))
        .cloned()
        .collect()
}

pub fn unconditional_scc(points: &[Point]) -> Option<VisionMatch> {
    let point = points
        .iter()
        .filter(|point| point.source == "scc" && point.measured && point.d_rel > 0.2)
        .min_by(|left, right| left.d_rel.total_cmp(&right.d_rel))?;
    Some(VisionMatch {
        point: Point {
            y_rel: 0.,
            yv_rel: 0.,
            ..point.clone()
        },
        probability: 0.,
        score: 1.,
        d_path: 0.,
    })
}

pub fn stationary_source_rank(point: &Point) -> u8 {
    if point.source == "frontRadar" {
        0
    } else if point.corner() {
        1
    } else if point.source == "scc" {
        2
    } else {
        3
    }
}

pub fn position_continuous(previous: &Point, previous_time: f64, point: &Point, now: f64) -> bool {
    let dt = now - previous_time;
    if !(0. ..=0.25).contains(&dt) {
        return false;
    }
    (point.d_rel - (previous.d_rel + previous.v_rel * dt)).abs()
        <= STATIONARY_LONGITUDINAL_CONTINUITY_M
        && (point.y_rel - (previous.y_rel + previous.yv_rel * dt)).abs()
            <= STATIONARY_LATERAL_CONTINUITY_M
}
