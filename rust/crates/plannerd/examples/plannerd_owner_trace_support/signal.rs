use openpilot_plannerd::{
    carrot::Signals,
    signal::{prepare::Geometry, priority::Candidate, transport::Context},
};
use serde::Deserialize;
use serde_json::Value;
#[derive(Deserialize)]
pub struct FixedGeometry {
    pub range: Option<[f64; 2]>,
    pub alignment: Option<[f64; 2]>,
    pub night_range: Option<[f64; 2]>,
}
impl Geometry for FixedGeometry {
    fn range(&mut self, _: [f64; 4], night: bool) -> Option<[f64; 2]> {
        if night {
            self.night_range
        } else {
            self.range
        }
    }
    fn alignment(&mut self, _: &Candidate) -> Option<[f64; 2]> {
        self.alignment
    }
    fn night(&self) -> bool {
        self.night_range.is_some()
    }
}
#[derive(Deserialize)]
pub struct Request {
    pub suspended: bool,
    pub start: Context,
    pub start_observation: Option<Value>,
    pub prepare: Context,
    pub prepare_observation: Option<Value>,
    pub prepare_actuate: bool,
    pub turning: bool,
    pub geometry: FixedGeometry,
}
impl Request {
    pub fn owner(&mut self) -> Signals<'_> {
        Signals {
            suspended: self.suspended,
            start: self.start.clone(),
            start_observation: self.start_observation.clone(),
            prepare: self.prepare.clone(),
            prepare_observation: self.prepare_observation.clone(),
            prepare_actuate: self.prepare_actuate,
            turning: self.turning,
            geometry: &mut self.geometry,
        }
    }
}
