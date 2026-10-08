use crate::{math::finite, model::VisionLead, path::Path, point::Point, Error, Lead};

pub fn from_point(point: &Point, d_path: f64, probability: f64, score: f64) -> Result<Lead, Error> {
    Ok(Lead {
        d_rel: point.d_rel,
        y_rel: point.y_rel,
        d_path,
        v_rel: point.v_rel,
        a_rel: point.a_rel,
        v_lead: point.v_lead,
        v_lead_k: point.v_lead,
        a_lead: point.a_lead,
        a_lead_k: point.a_lead,
        a_lead_tau: 1.5,
        j_lead: point.j_lead,
        v_lat: point.yv_rel,
        status: true,
        fcw: false,
        model_prob: probability,
        radar: true,
        radar_track_id: i32::try_from(point.track_id.0)
            .map_err(|_| Error::Contract("radar lead track ID exceeds cereal Int32"))?,
        score,
        ..Lead::default()
    })
}

pub fn from_vision(vision: &VisionLead, path: &Path, ego: [f64; 2]) -> Lead {
    let [speed, model_speed] = ego;
    let v_rel = vision.velocity - finite(model_speed, speed);
    let v_lead = speed + v_rel;
    Lead {
        d_rel: vision.d_rel,
        y_rel: vision.y_rel,
        d_path: path.project(vision.d_rel, vision.y_rel).d_path,
        v_rel,
        v_lead,
        v_lead_k: v_lead,
        a_lead: vision.acceleration,
        a_lead_k: vision.acceleration,
        a_lead_tau: 0.3,
        status: true,
        model_prob: vision.probability,
        ..Lead::default()
    }
}

pub fn duplicates_primary(lead: &Lead, primary: Option<&Lead>) -> bool {
    let Some(primary) = primary.filter(|lead| lead.status) else {
        return false;
    };
    if lead.radar_track_id >= 0
        && primary.radar_track_id >= 0
        && lead.radar_track_id == primary.radar_track_id
    {
        return true;
    }
    (lead.d_rel - primary.d_rel).abs() < 3.5 && (lead.y_rel - primary.y_rel).abs() < 1.8
}

#[derive(Clone, Copy)]
pub struct Entry {
    pub projected: bool,
    pub horizon: Option<f64>,
}

pub fn can_compete(lead: &Lead, primary: Option<&Lead>, entry: Entry) -> bool {
    let Some(primary) = primary.filter(|lead| lead.status) else {
        return true;
    };
    if !lead.d_rel.is_finite() || !primary.d_rel.is_finite() {
        return false;
    }
    if let Some(horizon) = entry.horizon.filter(|value| *value >= 0.) {
        let distance = lead.d_rel + lead.v_rel * horizon;
        let primary_distance = primary.d_rel + primary.v_rel * horizon;
        return distance.is_finite()
            && primary_distance.is_finite()
            && distance + 2. < primary_distance;
    }
    (lead.d_rel - primary.d_rel).abs() > 8. || entry.projected
}
