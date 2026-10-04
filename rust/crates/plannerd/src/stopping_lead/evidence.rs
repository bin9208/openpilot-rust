use crate::lead::Lead;
use openpilot_control_policy::math::{maximum, minimum};

#[derive(Debug)]
pub(super) struct Evidence {
    track_id: i32,
    since: f64,
    minimum: f64,
    maximum: f64,
    anchor: f64,
    held: bool,
    departure_since: Option<f64>,
    departed: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Observation {
    pub speed: f64,
    pub now: f64,
    pub fresh: bool,
}

pub(super) fn hold(
    evidence: Option<Evidence>,
    lead: &Lead,
    input: Observation,
) -> (Option<Evidence>, bool) {
    let valid = lead.status
        && lead.radar
        && lead.radar_track_id >= 0
        && [
            lead.d_rel,
            lead.v_rel,
            lead.v_lead,
            lead.a_lead_k,
            lead.j_lead,
        ]
        .iter()
        .all(|value| value.is_finite())
        && lead.d_rel > 0.2;
    if !valid || lead.v_lead < -0.30 || lead.a_lead_k < -0.5 {
        return (None, false);
    }
    let evidence = evidence.filter(|evidence| evidence.track_id == lead.radar_track_id);
    let quiet = input.speed <= 0.10 && lead.v_lead.abs() <= 0.30;
    let mut evidence = match evidence {
        Some(evidence) => evidence,
        None if quiet && input.fresh => Evidence {
            track_id: lead.radar_track_id,
            since: input.now,
            minimum: lead.d_rel,
            maximum: lead.d_rel,
            anchor: lead.d_rel,
            held: false,
            departure_since: None,
            departed: false,
        },
        None => return (None, false),
    };
    if evidence.departed {
        return (Some(evidence), false);
    }
    if !input.fresh {
        let held = evidence.held;
        return (Some(evidence), held);
    }
    if !evidence.held {
        evidence.minimum = minimum(evidence.minimum, lead.d_rel);
        evidence.maximum = maximum(evidence.maximum, lead.d_rel);
        if !quiet || evidence.maximum - evidence.minimum > 0.10 + 1e-6 {
            return (None, false);
        }
        if input.now - evidence.since < 0.20 - 1e-6 {
            return (Some(evidence), false);
        }
        evidence.held = true;
        evidence.anchor = lead.d_rel;
    }
    if lead.d_rel < evidence.anchor - 0.10 - 1e-6 {
        return (None, false);
    }
    let opening =
        lead.d_rel - evidence.anchor >= 0.15 - 1e-6 && lead.v_rel > 0. && lead.v_lead > 0.;
    if opening {
        let departure = *evidence.departure_since.get_or_insert(input.now);
        if input.now - departure >= 0.10 - 1e-6 {
            evidence.departed = true;
            return (Some(evidence), false);
        }
    } else {
        evidence.departure_since = None;
    }
    (Some(evidence), true)
}
