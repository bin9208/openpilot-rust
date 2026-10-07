use crate::point::{Identity, Point};
use indexmap::IndexMap;
use serde::Serialize;
use std::collections::HashSet;

pub type Matches = IndexMap<Identity, Point>;

pub fn match_cost(corner: &Point, front: &Point) -> Option<f64> {
    let d = (corner.d_rel - front.d_rel).abs();
    let y = (corner.y_rel - front.y_rel).abs();
    let v = (corner.v_lead - front.v_lead).abs();
    if d > 5. || y > 0.75 || v > 2. {
        return None;
    }
    Some(d / 5. + y / 0.75 + v / 2.)
}

fn hold_compatible(corner: &Point, front: &Point) -> bool {
    (corner.d_rel - front.d_rel).abs() <= 12.
        && (corner.y_rel - front.y_rel).abs() <= 2.
        && (corner.v_lead - front.v_lead).abs() <= 2.
}

fn with_front(point: &Point, front: &Point) -> Point {
    Point {
        track_id: point.track_id,
        source: point.source.clone(),
        d_rel: front.d_rel,
        y_rel: point.y_rel,
        v_rel: front.v_rel,
        a_rel: front.a_rel,
        yv_rel: point.yv_rel,
        v_lead: front.v_lead,
        a_lead: front.a_lead,
        j_lead: front.j_lead,
        measured: point.measured && front.measured,
        kinematics_source: Some(front.source.clone()),
        kinematics_track_id: Some(front.track_id),
        radar_track_state: 0,
    }
}

pub fn prefer_front(point: &Point, matches: &Matches) -> Point {
    if !point.corner() {
        return point.clone();
    }
    matches
        .get(&point.identity())
        .map_or_else(|| point.clone(), |front| with_front(point, front))
}

pub fn prefer_front_current(point: &Point, points: &[Point]) -> Point {
    if !point.corner() {
        return point.clone();
    }
    let Some((_, front)) = points
        .iter()
        .filter(|front| front.source == "frontRadar")
        .filter_map(|front| match_cost(point, front).map(|cost| (cost, front)))
        .min_by(|left, right| {
            (left.0, left.1.track_id)
                .partial_cmp(&(right.0, right.1.track_id))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    else {
        return point.clone();
    };
    let reverse = points
        .iter()
        .filter(|corner| corner.corner())
        .filter_map(|corner| match_cost(corner, front).map(|cost| (cost, corner)))
        .min_by(|left, right| {
            (left.0, left.1.track_id)
                .partial_cmp(&(right.0, right.1.track_id))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    if reverse.is_some_and(|(_, corner)| corner.identity() == point.identity()) {
        with_front(point, front)
    } else {
        point.clone()
    }
}

#[derive(Clone, Default, Serialize)]
pub struct Associator {
    pub pairs: IndexMap<Identity, Identity>,
}

impl Associator {
    pub fn reset(&mut self) {
        self.pairs.clear();
    }

    pub fn update(&mut self, points: &[Point]) -> Matches {
        let by_identity: IndexMap<_, _> = points
            .iter()
            .map(|point| (point.identity(), point))
            .collect();
        let mut best_front: IndexMap<Identity, (f64, Identity)> = IndexMap::new();
        let mut best_corner: IndexMap<Identity, (f64, Identity)> = IndexMap::new();
        for corner in points.iter().filter(|point| point.corner()) {
            for front in points.iter().filter(|point| point.source == "frontRadar") {
                let Some(cost) = match_cost(corner, front) else {
                    continue;
                };
                let corner_id = corner.identity();
                let front_id = front.identity();
                if best_front
                    .get(&corner_id)
                    .is_none_or(|current| (cost, front_id.1) < (current.0, current.1 .1))
                {
                    best_front.insert(corner_id.clone(), (cost, front_id.clone()));
                }
                if best_corner
                    .get(&front_id)
                    .is_none_or(|current| (cost, corner_id.1) < (current.0, current.1 .1))
                {
                    best_corner.insert(front_id, (cost, corner_id));
                }
            }
        }
        let mut matches = Matches::new();
        for (corner, (_, front)) in &best_front {
            if best_corner
                .get(front)
                .is_some_and(|(_, selected)| selected == corner)
            {
                if let Some(point) = by_identity.get(front) {
                    matches.insert(corner.clone(), (*point).clone());
                }
            }
        }
        let mut remembered = IndexMap::new();
        for (corner_id, front_id) in &self.pairs {
            if let (Some(corner), Some(front)) =
                (by_identity.get(corner_id), by_identity.get(front_id))
            {
                if hold_compatible(corner, front) {
                    remembered.insert(corner_id.clone(), front_id.clone());
                }
            }
        }
        for (corner, front) in &matches {
            remembered.insert(corner.clone(), front.identity());
        }
        let mut used: HashSet<_> = matches.values().map(Point::identity).collect();
        let mut sorted: Vec<_> = remembered.iter().collect();
        sorted.sort_by(|left, right| left.0.cmp(right.0));
        for (corner_id, front_id) in sorted {
            if matches.contains_key(corner_id) || used.contains(front_id) {
                continue;
            }
            if let (Some(corner), Some(front)) =
                (by_identity.get(corner_id), by_identity.get(front_id))
            {
                if hold_compatible(corner, front) {
                    matches.insert(corner_id.clone(), (*front).clone());
                    used.insert(front_id.clone());
                }
            }
        }
        self.pairs = remembered;
        matches
    }
}
