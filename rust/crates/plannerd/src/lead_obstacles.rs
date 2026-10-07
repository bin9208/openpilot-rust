use crate::lead::Lead;
use openpilot_control_policy::math::{clip, maximum, minimum};

#[derive(Clone, Copy, Debug)]
pub struct FollowGeometry {
    pub speed: f64,
    pub follow_time: f64,
    pub stop_distance: f64,
}

pub fn cutout_relief<const N: usize>(
    lead: &Lead,
    geometry: FollowGeometry,
    horizons: &[f64; N],
) -> [f64; N] {
    let FollowGeometry {
        speed,
        follow_time,
        stop_distance,
    } = geometry;
    let exit_time = lead.cut_out_time;
    let confidence = lead.cut_out_confidence;
    if !lead.status
        || !lead.radar
        || lead.radar_track_id < 0
        || ![
            exit_time,
            confidence,
            speed,
            follow_time,
            stop_distance,
            lead.d_rel,
            lead.v_rel,
            lead.v_lead,
            lead.a_lead_k,
        ]
        .iter()
        .all(|value| value.is_finite())
        || exit_time <= 0.
        || exit_time > 2.5
        || confidence <= 0.
        || confidence > 1.
        || speed < 5.
        || lead.v_lead <= 4.
        || lead.a_lead_k < -2.5
        || follow_time <= 0.
    {
        return [0.; N];
    }
    let clearance = exit_time + 0.30;
    let remaining = lead.d_rel
        + minimum(0., lead.v_rel) * clearance
        + 0.5 * (minimum(-0.5, lead.a_lead_k) - 0.5) * clearance.powi(2);
    if remaining <= maximum(6., stop_distance) {
        return [0.; N];
    }
    let credit = minimum(8., minimum(0.5, 0.5 * follow_time) * speed) * confidence;
    horizons.map(|time| credit * clip((time - clearance) / 0.50, 0., 1.))
}

pub fn predecel_limit(lead: &Lead) -> Option<f64> {
    if !lead.status
        || !lead.radar
        || lead.score < 0.15
        || !lead.d_rel.is_finite()
        || !lead.v_rel.is_finite()
        || lead.d_rel <= 0.
        || lead.v_rel >= 0.
    {
        return None;
    }
    let ttc = minimum(8., maximum(2.5, lead.d_rel / maximum(-lead.v_rel, 0.1)));
    let fraction = (ttc - 2.5) / (8. - 2.5);
    Some(-0.65 + fraction * (-0.25 - (-0.65)))
}

pub fn apply_predecel(maximum_accel: f64, desired_accel: f64, limit: Option<f64>) -> f64 {
    let (requested, step) = match limit {
        Some(limit) => (minimum(maximum_accel, limit), 0.15),
        None => (maximum_accel, 0.05),
    };
    maximum(requested, desired_accel - step)
}
