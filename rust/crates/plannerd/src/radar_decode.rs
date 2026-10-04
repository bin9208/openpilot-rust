use crate::{
    lead::Lead,
    radar::{Radar, RadarPoint, RadarSource},
    Error,
};
use openpilot_cereal::{car_capnp::radar_data, log_capnp::radar_state};

pub fn lead(value: radar_state::lead_data::Reader<'_>) -> Lead {
    Lead {
        d_rel: f64::from(value.get_d_rel()),
        y_rel: f64::from(value.get_y_rel()),
        v_rel: f64::from(value.get_v_rel()),
        a_rel: f64::from(value.get_a_rel()),
        v_lead: f64::from(value.get_v_lead()),
        d_path: f64::from(value.get_d_path()),
        v_lat: f64::from(value.get_v_lat()),
        v_lead_k: f64::from(value.get_v_lead_k()),
        a_lead_k: f64::from(value.get_a_lead_k()),
        fcw: value.get_fcw(),
        status: value.get_status(),
        a_lead_tau: f64::from(value.get_a_lead_tau()),
        model_prob: f64::from(value.get_model_prob()),
        radar: value.get_radar(),
        radar_track_id: value.get_radar_track_id(),
        a_lead: f64::from(value.get_a_lead()),
        j_lead: f64::from(value.get_j_lead()),
        score: f64::from(value.get_score()),
        cut_out_time: f64::from(value.get_cut_out_time()),
        cut_out_confidence: f64::from(value.get_cut_out_confidence()),
    }
}

pub fn radar(value: radar_state::Reader<'_>) -> Result<Radar, Error> {
    Ok(Radar {
        lead_one: lead(value.get_lead_one()?),
        lead_two: lead(value.get_lead_two()?),
        lead_cut_in_risk: lead(value.get_lead_cut_in_risk()?),
    })
}

pub fn points(value: radar_data::Reader<'_>) -> Result<Vec<RadarPoint>, Error> {
    Ok(value
        .get_points()?
        .iter()
        .map(|point| RadarPoint {
            track_id: point.get_track_id(),
            measured: point.get_measured(),
            d_rel: f64::from(point.get_d_rel()),
            v_rel: f64::from(point.get_v_rel()),
            a_rel: f64::from(point.get_a_rel()),
            a_lead: f64::from(point.get_a_lead()),
            j_lead: f64::from(point.get_j_lead()),
            radar_source: RadarSource(match point.get_radar_source() {
                Ok(source) => source.into(),
                Err(capnp::NotInSchema(raw)) => raw,
            }),
        })
        .collect())
}
