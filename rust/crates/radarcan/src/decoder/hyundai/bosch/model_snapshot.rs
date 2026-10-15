//! Owned immutable copy shared by the Bosch geometry consumers.
use super::{
    birth_b5::{Model, Number},
    physical::VisionCue,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Deserialize)]
pub struct Lead {
    #[serde(default)]
    pub x: Vec<Number>,
    #[serde(default)]
    pub y: Vec<Number>,
    pub prob: Number,
}
#[derive(Clone, Default, Deserialize)]
pub struct Input {
    #[serde(flatten)]
    pub geometry: Model,
    #[serde(rename = "leadsV3", default)]
    pub leads: Vec<Lead>,
}
#[derive(Clone, Serialize)]
pub struct Cue {
    pub d_rel: Number,
    pub y_rel: Number,
    pub probability: Number,
    pub distance_tolerance_m: f64,
    pub lateral_tolerance_m: f64,
}
#[derive(Clone, Serialize)]
pub struct Snapshot {
    #[serde(flatten)]
    geometry: Model,
    cues: Vec<Cue>,
}
impl Snapshot {
    pub fn new(input: &Input) -> Self {
        let cues = input
            .leads
            .first()
            .and_then(|lead| {
                Some(Cue {
                    d_rel: Number(lead.x.first()?.0 - 1.52),
                    y_rel: Number(-lead.y.first()?.0),
                    probability: lead.prob,
                    distance_tolerance_m: 8.,
                    lateral_tolerance_m: 1.5,
                })
            })
            .into_iter()
            .collect();
        Self {
            geometry: input.geometry.clone(),
            cues,
        }
    }
    pub fn geometry(&self) -> &Model {
        &self.geometry
    }
    pub fn cues(&self) -> impl Iterator<Item = VisionCue> + '_ {
        self.cues.iter().map(|cue| VisionCue {
            d_rel: cue.d_rel.0,
            y_rel: cue.y_rel.0,
            probability: cue.probability.0,
            distance_tolerance_m: cue.distance_tolerance_m,
            lateral_tolerance_m: cue.lateral_tolerance_m,
        })
    }
}
