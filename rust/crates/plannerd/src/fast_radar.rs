use crate::{
    lead::Lead,
    lead_dynamics::LeadAccelTau,
    number,
    radar::{Radar, RadarPoint, RadarSource},
    Error,
};
use openpilot_control_policy::math::maximum;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod refresh;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FastReason {
    Inactive,
    NotRadarLead,
    SelectionPending,
    SelectionUnstable,
    TrackMissing,
    TrackUnmeasured,
    NonFinite,
    InvalidDistance,
    DistanceDiscontinuity,
    VelocityDiscontinuity,
    Active,
    RadarStateInvalid,
    LiveTracksInvalid,
    SelectionStale,
}

impl FastReason {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::NotRadarLead => "notRadarLead",
            Self::SelectionPending => "selectionPending",
            Self::SelectionUnstable => "selectionUnstable",
            Self::TrackMissing => "trackMissing",
            Self::TrackUnmeasured => "trackUnmeasured",
            Self::NonFinite => "nonFinite",
            Self::InvalidDistance => "invalidDistance",
            Self::DistanceDiscontinuity => "distanceDiscontinuity",
            Self::VelocityDiscontinuity => "velocityDiscontinuity",
            Self::Active => "active",
            Self::RadarStateInvalid => "radarStateInvalid",
            Self::LiveTracksInvalid => "liveTracksInvalid",
            Self::SelectionStale => "selectionStale",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FastInput {
    pub ego_speed: f64,
    pub radar_mono_ns: u64,
    pub live_mono_ns: u64,
    pub radar_valid: bool,
    pub live_valid: bool,
}

#[derive(Debug, Serialize)]
pub struct FastResult {
    pub radar_state: Radar,
    pub lead_mask: u8,
    pub lead_one_track_id: i32,
    pub selection_age_s: f64,
    pub lead_one_reason: FastReason,
}

struct Selection {
    signature: (bool, bool, i32),
    consecutive: u64,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            signature: (false, false, -1),
            consecutive: 0,
        }
    }
}

pub struct FastRadarOverlay {
    front_delay: f64,
    selections: [Selection; 2],
    last_radar_ns: u64,
    acceleration_tau: BTreeMap<(RadarSource, i32), LeadAccelTau>,
}

impl FastRadarOverlay {
    pub fn new(front_delay: f64) -> Self {
        Self {
            front_delay: maximum(0., front_delay),
            selections: std::array::from_fn(|_| Selection::default()),
            last_radar_ns: 0,
            acceleration_tau: BTreeMap::new(),
        }
    }

    pub fn observe(&mut self, radar: &Radar, mono_ns: u64, valid: bool) {
        if mono_ns <= self.last_radar_ns {
            return;
        }
        self.last_radar_ns = mono_ns;
        let mut active = [None; 2];
        for (index, (selection, lead)) in self
            .selections
            .iter_mut()
            .zip([&radar.lead_one, &radar.lead_two])
            .enumerate()
        {
            let signature = if valid {
                lead.signature()
            } else {
                (false, false, -1)
            };
            if signature == selection.signature {
                selection.consecutive = selection.consecutive.saturating_add(1);
            } else {
                selection.signature = signature;
                selection.consecutive = 1;
            }
            if signature.0 && signature.1 && signature.2 >= 0 {
                active[index] = Some(signature.2);
            }
        }
        self.acceleration_tau
            .retain(|(_, id), _| active.contains(&Some(*id)));
    }

    fn rejection(&self, role: usize, lead: &Lead) -> Option<FastReason> {
        if !lead.status || !lead.radar || lead.radar_track_id < 0 {
            return Some(FastReason::NotRadarLead);
        }
        let selected = &self.selections[role];
        if selected.signature != lead.signature() {
            return Some(FastReason::SelectionPending);
        }
        if selected.consecutive < 2 {
            return Some(FastReason::SelectionUnstable);
        }
        None
    }

    pub fn lead_one_ready(&self, radar: &Radar) -> bool {
        self.rejection(0, &radar.lead_one).is_none()
    }

    pub fn build(
        &mut self,
        radar: &Radar,
        points: &[RadarPoint],
        input: FastInput,
    ) -> Result<FastResult, Error> {
        let age = number::age_seconds(input.live_mono_ns, input.radar_mono_ns)?;
        let age_valid = (0. ..=0.15).contains(&age);
        if !input.radar_valid || !input.live_valid || !age_valid {
            for tau in self.acceleration_tau.values_mut() {
                tau.clear_evidence();
            }
        }
        let mut result = FastResult {
            radar_state: *radar,
            lead_mask: 0,
            lead_one_track_id: -1,
            selection_age_s: age,
            lead_one_reason: FastReason::NotRadarLead,
        };
        let rejection = if !input.radar_valid {
            Some(FastReason::RadarStateInvalid)
        } else if !input.live_valid {
            Some(FastReason::LiveTracksInvalid)
        } else if !age_valid {
            Some(FastReason::SelectionStale)
        } else {
            None
        };
        if let Some(reason) = rejection {
            result.lead_one_reason = reason;
            return Ok(result);
        }
        let sample_time = number::timestamp_seconds(input.live_mono_ns)?;
        for (role, lead) in [
            &mut result.radar_state.lead_one,
            &mut result.radar_state.lead_two,
        ]
        .into_iter()
        .enumerate()
        {
            let point = match u64::try_from(lead.radar_track_id) {
                Ok(id) => {
                    let mut matches = points.iter().filter(|point| point.track_id == id);
                    let first = matches.next();
                    if matches.next().is_some() {
                        None
                    } else {
                        first
                    }
                }
                Err(_) => None,
            };
            let (active, reason) = self.refresh(
                lead,
                refresh::Refresh {
                    role,
                    point,
                    ego_speed: input.ego_speed,
                    age,
                    sample_time,
                },
            )?;
            if active {
                result.lead_mask |= 1 << role;
            } else {
                for ((_, id), tau) in &mut self.acceleration_tau {
                    if *id == lead.radar_track_id {
                        tau.clear_evidence();
                    }
                }
            }
            if role == 0 {
                result.lead_one_reason = reason;
            }
        }
        if result.lead_mask & 1 != 0 {
            result.lead_one_track_id = result.radar_state.lead_one.radar_track_id;
        }
        Ok(result)
    }
}
