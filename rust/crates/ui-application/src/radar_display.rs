//! Display-only ARS points never substitute for control tracks.
use crate::Error;
use openpilot_cereal::{car_capnp::radar_data, log_capnp::event};
use openpilot_messaging::state::State;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Point {
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_lead_k: f64,
    pub v_lat: f64,
    pub radar: bool,
    pub model_prob: f64,
}
#[derive(Debug, Deserialize)]
pub struct RawPoint {
    pub track_id: u64,
    pub measured: bool,
    pub front_radar: bool,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_lead: f64,
    pub yv_rel: f64,
}
#[derive(Debug, Deserialize)]
pub struct Input {
    pub supported: bool,
    pub seen: bool,
    pub alive: bool,
    pub valid: bool,
    pub receive_frame: i64,
    pub receive_time: f64,
    pub publication_ns: u64,
    pub errors: bool,
    pub points: Vec<RawPoint>,
}
pub fn select(input: &Input, show: i32, now: f64, started_frame: i64) -> Option<Vec<Point>> {
    if show < 3 || !input.supported {
        return None;
    }
    let fresh = |stamp: f64| stamp.is_finite() && stamp <= now && now <= stamp + 0.2;
    #[allow(clippy::cast_precision_loss)]
    let published = input.publication_ns as f64 * 1e-9;
    if !now.is_finite()
        || !input.seen
        || !input.alive
        || !input.valid
        || input.receive_frame < started_frame
        || input.publication_ns == 0
        || !fresh(input.receive_time)
        || !fresh(published)
        || input.errors
        || input.points.len() > 20
    {
        return Some(Vec::new());
    }
    let mut points = Vec::with_capacity(input.points.len());
    let mut identities = Vec::with_capacity(input.points.len());
    for point in &input.points {
        if !point.measured
            || !point.front_radar
            || ![point.d_rel, point.y_rel, point.v_lead, point.yv_rel]
                .iter()
                .all(|x| x.is_finite())
            || point.d_rel <= 0.
            || point.d_rel > (4095. - 160.) / 16.
            || !(-2048. * 0.015..=2048. * 0.015).contains(&point.y_rel)
        {
            continue;
        }
        if point.track_id >= 1 << 31 || identities.contains(&point.track_id) {
            return Some(Vec::new());
        }
        identities.push(point.track_id);
        points.push(Point {
            d_rel: point.d_rel,
            y_rel: point.y_rel,
            v_lead_k: point.v_lead,
            v_lat: point.yv_rel,
            radar: true,
            model_prob: 0.03,
        });
    }
    Some(points)
}
pub fn from_messages(
    messages: &State,
    show: i32,
    now: f64,
    started_frame: i64,
) -> Result<Option<Vec<Point>>, Error> {
    if show < 3 {
        return Ok(None);
    }
    let Ok(topic) = messages.topic("liveTracks") else {
        return Ok(None);
    };
    let event::Which::LiveTracks(data) = topic.event()?.which()? else {
        return Ok(None);
    };
    let data = data?;
    if !data.get_has_display_data() {
        return Ok(None);
    }
    let errors = data.get_errors()?;
    let raw = data.get_display_points()?;
    if raw.len() > 20 {
        return Ok(Some(Vec::new()));
    }
    let points = raw
        .iter()
        .map(|point| RawPoint {
            track_id: point.get_track_id(),
            measured: point.get_measured(),
            front_radar: point.get_radar_source()
                == Ok(radar_data::radar_point::RadarSource::FrontRadar),
            d_rel: f64::from(point.get_d_rel()),
            y_rel: f64::from(point.get_y_rel()),
            v_lead: f64::from(point.get_v_lead()),
            yv_rel: f64::from(point.get_yv_rel()),
        })
        .collect();
    Ok(select(
        &Input {
            supported: true,
            seen: topic.seen,
            alive: topic.alive,
            valid: topic.valid,
            receive_frame: topic.receive_frame,
            receive_time: topic.receive_time,
            publication_ns: topic.log_mono_time,
            errors: errors.get_can_error()
                || errors.get_radar_fault()
                || errors.get_wrong_config()
                || errors.get_radar_unavailable_temporary(),
            points,
        },
        show,
        now,
        started_frame,
    ))
}
pub fn render_points(
    messages: &State,
    radar: openpilot_cereal::log_capnp::radar_state::Reader<'_>,
    show: i32,
    now: f64,
    started_frame: i64,
) -> Result<Vec<Point>, Error> {
    if let Some(points) = from_messages(messages, show, now, started_frame)? {
        return Ok(points);
    }
    let mut points = Vec::new();
    for group in [
        radar.get_leads_left()?,
        radar.get_leads_right()?,
        radar.get_leads_center()?,
    ] {
        points.extend(group.iter().map(|lead| Point {
            d_rel: f64::from(lead.get_d_rel()),
            y_rel: f64::from(lead.get_y_rel()),
            v_lead_k: f64::from(lead.get_v_lead_k()),
            v_lat: f64::from(lead.get_v_lat()),
            radar: lead.get_radar(),
            model_prob: f64::from(lead.get_model_prob()),
        }));
    }
    Ok(points)
}
