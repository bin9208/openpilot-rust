mod handoff;
mod shadow;
mod tracker;
pub use handoff::PrimaryHandoff;
pub use shadow::StationaryShadow;
pub use tracker::LeadTwoTracker;

use crate::{lead::duplicates_primary, point::TrackId, Lead};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Identity(pub String, pub TrackId, pub u64);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Candidate {
    pub lead: Lead,
    pub source: String,
    pub track_id: TrackId,
    pub continuity_id: u64,
    pub retainable: bool,
    pub confirmed_cutin: bool,
    #[serde(default)]
    pub confirmed_stationary_shadow: bool,
    #[serde(default)]
    pub allow_low_speed: bool,
}

impl Candidate {
    pub fn identity(&self) -> Identity {
        Identity(self.source.clone(), self.track_id, self.continuity_id)
    }
}

#[derive(Clone, Default, Serialize)]
pub struct Selection {
    pub cutins: Vec<Lead>,
    pub lead_two: Option<Lead>,
}

#[derive(Default)]
pub struct Exceptions {
    pub stopped: HashSet<i32>,
    pub farther: HashSet<i32>,
    pub proximity: HashSet<i32>,
}

pub fn selected_indices(
    primary: Option<&Lead>,
    candidates: &[&Candidate],
    exceptions: &Exceptions,
) -> Vec<usize> {
    let primary_distance = primary
        .filter(|lead| lead.status && lead.d_rel.is_finite())
        .map_or(f64::INFINITY, |lead| lead.d_rel);
    let mut indices: Vec<_> = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            let lead = &candidate.lead;
            lead.status
                && lead.radar
                && 0.8 < lead.d_rel
                && lead.d_rel <= 80.
                && (lead.d_rel < primary_distance
                    || exceptions.farther.contains(&lead.radar_track_id))
                && (lead.v_lead > 2.5 || exceptions.stopped.contains(&lead.radar_track_id))
                && (!duplicates_primary(lead, primary)
                    || (exceptions.proximity.contains(&lead.radar_track_id)
                        && primary
                            .is_some_and(|primary| (lead.v_lead - primary.v_lead).abs() > 2.5)))
        })
        .map(|(index, _)| index)
        .collect();
    indices.sort_by(|left, right| {
        candidates[*left]
            .lead
            .d_rel
            .total_cmp(&candidates[*right].lead.d_rel)
    });
    indices
}

pub fn position_continuous(
    previous: Option<(&Lead, f64)>,
    lead: &Lead,
    now: f64,
    hold: f64,
) -> bool {
    let Some((previous, time)) = previous else {
        return true;
    };
    let dt = now - time;
    if dt < 0. || dt > hold {
        return false;
    }
    (lead.d_rel - (previous.d_rel + previous.v_rel * dt)).abs() <= 2.25
        && (lead.y_rel - (previous.y_rel + previous.v_lat * dt)).abs() <= 1.25
}
