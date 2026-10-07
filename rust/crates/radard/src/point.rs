use crate::math::finite;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(transparent)]
pub struct TrackId(pub i128);

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Identity(pub String, pub TrackId);

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Point {
    pub track_id: TrackId,
    pub source: String,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
    pub a_rel: f64,
    pub yv_rel: f64,
    pub v_lead: f64,
    pub a_lead: f64,
    pub j_lead: f64,
    pub measured: bool,
    pub radar_track_state: i32,
    pub kinematics_source: Option<String>,
    pub kinematics_track_id: Option<TrackId>,
}

impl Point {
    pub fn identity(&self) -> Identity {
        Identity(self.source.clone(), self.track_id)
    }
    pub fn corner(&self) -> bool {
        self.source.starts_with("corner")
    }
    pub fn aligned(&self, ego_speed: f64, delta: f64) -> Self {
        let v_rel = finite(self.v_rel, 0.);
        let yv_rel = finite(self.yv_rel, 0.);
        Self {
            track_id: self.track_id,
            source: self.source.rsplit('.').next().unwrap_or("").to_owned(),
            d_rel: finite(self.d_rel, 0.) + v_rel * delta,
            y_rel: finite(self.y_rel, 0.) + yv_rel * delta,
            v_rel,
            a_rel: finite(self.a_rel, 0.),
            yv_rel,
            v_lead: ego_speed + v_rel,
            a_lead: finite(self.a_lead, 0.),
            j_lead: finite(self.j_lead, 0.),
            measured: true,
            radar_track_state: self.radar_track_state,
            kinematics_source: None,
            kinematics_track_id: None,
        }
    }
}

pub fn velocity_in_ego_frame(point: &Point, yaw_rate: f64) -> [f64; 2] {
    let yaw = finite(yaw_rate, 0.);
    [
        finite(point.v_lead, 0.) - yaw * finite(point.y_rel, 0.),
        finite(point.yv_rel, 0.) + yaw * finite(point.d_rel, 0.),
    ]
}
