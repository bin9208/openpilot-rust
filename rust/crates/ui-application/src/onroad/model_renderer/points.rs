use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelPoint(pub [f32; 3]);

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SamplePoint(pub [f64; 3]);

impl From<ModelPoint> for SamplePoint {
    fn from(value: ModelPoint) -> Self {
        Self(value.0.map(f64::from))
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(transparent)]
pub struct ScreenPoint(pub [f64; 2]);
