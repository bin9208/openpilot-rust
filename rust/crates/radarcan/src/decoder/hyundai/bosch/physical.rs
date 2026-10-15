//! Distinct physical identity anchors and downstream publication surfaces.
use super::tracking::Track;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Serialize, Deserialize)]
pub struct Surface {
    pub raw_track_id: u32,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
    pub lateral_estimate: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Object {
    pub physical_track_id: u32,
    pub timestamp_ns: i64,
    pub members: Vec<Track>,
    pub representative_raw_track_id: u32,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
    pub oem_selected: bool,
    pub vision_supported: bool,
    pub age_scans: usize,
    pub grouping_evidence: String,
    pub published_surface: Option<Surface>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct VisionCue {
    pub d_rel: f64,
    pub y_rel: f64,
    pub probability: f64,
    pub distance_tolerance_m: f64,
    pub lateral_tolerance_m: f64,
}
impl Object {
    pub fn published(&self) -> (u32, f64, f64, f64) {
        self.published_surface.as_ref().map_or(
            (
                self.representative_raw_track_id,
                self.d_rel,
                self.y_rel,
                self.v_rel,
            ),
            |surface| {
                (
                    surface.raw_track_id,
                    surface.d_rel,
                    surface.y_rel,
                    surface.v_rel,
                )
            },
        )
    }
}
#[derive(Clone, Copy)]
pub struct Anchor {
    pub representative_raw_track_id: u32,
    pub v_rel: f64,
    pub predicted_xy: (f64, f64),
}
/// Choose a member using an existing physical object's continuity anchor.
pub fn representative<'a>(
    candidates: &'a [Track],
    raw_index: &BTreeMap<u32, usize>,
    vision_supported: &[bool],
    oem_slot: Option<usize>,
    prior: Option<(&Object, (f64, f64))>,
) -> Result<&'a Track, &'static str> {
    representative_from_anchor(
        candidates,
        raw_index,
        vision_supported,
        oem_slot,
        prior.map(|(object, xy)| Anchor {
            representative_raw_track_id: object.representative_raw_track_id,
            v_rel: object.v_rel,
            predicted_xy: xy,
        }),
    )
}
/// Choose one whole member tuple; camera/OEM support never creates identity.
pub fn representative_from_anchor<'a>(
    candidates: &'a [Track],
    raw_index: &BTreeMap<u32, usize>,
    vision_supported: &[bool],
    oem_slot: Option<usize>,
    prior: Option<Anchor>,
) -> Result<&'a Track, &'static str> {
    let first = candidates.first().ok_or("min() arg is an empty sequence")?;
    if candidates.len() == 1 {
        return Ok(first);
    }
    let median = if prior.is_none() {
        let mut ordered: Vec<_> = candidates.iter().map(|m| m.detection.d_rel).collect();
        ordered.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let middle = ordered.len() / 2;
        if ordered.len() % 2 != 0 {
            ordered[middle]
        } else {
            (ordered[middle - 1] + ordered[middle]) / 2.
        }
    } else {
        0.
    };
    let cost = |member: &Track| {
        let point = &member.detection;
        let (continuity, changed) = prior.map_or(((point.d_rel - median).abs(), false), |prior| {
            let (px, py) = prior.predicted_xy;
            (
                (point.d_rel - px).abs()
                    + 0.5 * (point.y_rel - py).abs()
                    + 0.5 * (point.v_rel - prior.v_rel).abs(),
                member.raw_track_id != prior.representative_raw_track_id,
            )
        });
        (
            continuity,
            changed,
            !vision_supported[raw_index[&member.raw_track_id]],
            Some(point.slot) != oem_slot,
            std::cmp::Reverse(member.age_scans),
            member.raw_track_id,
        )
    };
    let mut best = first;
    let mut best_cost = cost(first);
    for member in &candidates[1..] {
        let value = cost(member);
        // Python tuple ordering stops at an unequal NaN instead of comparing
        // later fields. Finite ties retain all source support/age/ID criteria.
        let less = if value.0 != best_cost.0 {
            value.0 < best_cost.0
        } else {
            (value.1, value.2, value.3, value.4, value.5)
                < (
                    best_cost.1,
                    best_cost.2,
                    best_cost.3,
                    best_cost.4,
                    best_cost.5,
                )
        };
        if less {
            best = member;
            best_cost = value;
        }
    }
    Ok(best)
}
