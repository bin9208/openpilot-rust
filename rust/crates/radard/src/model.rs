use crate::math::finite;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct VisionLead {
    pub probability: f64,
    pub d_rel: f64,
    pub y_rel: f64,
    pub velocity: f64,
    pub x_std: f64,
    pub y_std: f64,
    pub v_std: f64,
    pub acceleration: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelLead {
    pub probability: f64,
    #[serde(with = "crate::wire_float::optional")]
    pub x: Option<Vec<f64>>,
    #[serde(with = "crate::wire_float::optional")]
    pub y: Option<Vec<f64>>,
    #[serde(with = "crate::wire_float::optional")]
    pub v: Option<Vec<f64>>,
    #[serde(with = "crate::wire_float::optional")]
    pub a: Option<Vec<f64>>,
    #[serde(rename = "xStd", with = "crate::wire_float::optional")]
    pub x_std: Option<Vec<f64>>,
    #[serde(rename = "yStd", with = "crate::wire_float::optional")]
    pub y_std: Option<Vec<f64>>,
    #[serde(rename = "vStd", with = "crate::wire_float::optional")]
    pub v_std: Option<Vec<f64>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Position {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Model {
    pub position: Position,
    pub velocity: Vec<f64>,
    pub lane_probabilities: Vec<f64>,
    pub leads: Vec<ModelLead>,
}

fn first(values: &[f64], fallback: f64) -> f64 {
    values
        .first()
        .map_or(fallback, |value| finite(*value, fallback))
}

impl Model {
    pub fn path(&self) -> Vec<[f64; 2]> {
        self.position
            .x
            .iter()
            .zip(&self.position.y)
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .map(|(x, y)| [*x, *y])
            .collect()
    }
    pub fn ego_speed(&self, fallback: f64) -> f64 {
        first(&self.velocity, fallback)
    }
    pub fn primary_vision(&self) -> Option<VisionLead> {
        let lead = self.leads.first()?;
        let (x, y, v) = (lead.x.as_deref()?, lead.y.as_deref()?, lead.v.as_deref()?);
        if x.is_empty() || y.is_empty() || v.is_empty() {
            return None;
        }
        let d_rel = first(x, 0.) - 1.52;
        if d_rel <= 0.5 {
            return None;
        }
        Some(VisionLead {
            probability: finite(lead.probability, 0.),
            d_rel,
            y_rel: -first(y, 0.),
            velocity: first(v, 0.),
            x_std: first(lead.x_std.as_deref().unwrap_or(&[]), 1.),
            y_std: first(lead.y_std.as_deref().unwrap_or(&[]), 1.),
            v_std: first(lead.v_std.as_deref().unwrap_or(&[]), 1.),
            acceleration: first(lead.a.as_deref().unwrap_or(&[]), 0.),
        })
    }
}
