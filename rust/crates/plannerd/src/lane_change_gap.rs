use crate::lead::Lead;
use serde::{Deserialize, Serialize};

mod credit;
pub use credit::CreditInput;
mod tracker;
pub use tracker::{Input, Tracker};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct GapLead {
    pub id: i32,
    pub distance: f64,
    pub lateral: f64,
    pub relative_speed: f64,
    pub speed: f64,
    pub acceleration: f64,
}

impl GapLead {
    pub fn read(lead: Option<&Lead>) -> Option<Self> {
        let lead = lead?;
        let values = [
            lead.d_rel,
            lead.y_rel,
            lead.v_rel,
            lead.v_lead,
            lead.a_lead_k,
        ];
        let valid = lead.status
            && lead.radar
            && lead.radar_track_id >= 0
            && values.iter().all(|v| v.is_finite())
            && -30. < lead.d_rel
            && lead.d_rel < 160.
            && lead.y_rel.abs() <= 12.
            && (0. ..=70.).contains(&lead.v_lead)
            && lead.v_rel.abs() <= 70.
            && (-10. ..=5.).contains(&lead.a_lead_k);
        if !valid {
            return None;
        }
        Some(Self {
            id: lead.radar_track_id,
            distance: lead.d_rel,
            lateral: lead.y_rel,
            relative_speed: lead.v_rel,
            speed: lead.v_lead,
            acceleration: lead.a_lead_k,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Plan {
    pub active: bool,
    pub primary_id: i32,
    pub targets: Vec<GapLead>,
    pub clearance: f64,
    pub confidence: f64,
    pub reason: Reason,
    pub entry_ids: Vec<i32>,
    pub selected_ids: Vec<i32>,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            active: false,
            primary_id: -1,
            targets: Vec::new(),
            clearance: 0.,
            confidence: 0.,
            reason: Reason::Inactive,
            entry_ids: Vec::new(),
            selected_ids: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    Inactive,
    LegacySideInput,
    InvalidInput,
    PoseOrSpeed,
    SelectedLeadsChanged,
    TrackDiscontinuity,
    SessionLimit,
    PrimaryChanged,
    DestinationUnconfirmed,
    ConfirmMotion,
    UnconfirmedMotion,
    InvalidPath,
    NoClearance,
    PathReentry,
    ConfirmedDeparture,
}

fn check_time(index: u32) -> f64 {
    f64::from(index) * (3. / 60.)
}
