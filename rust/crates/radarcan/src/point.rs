use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    #[default]
    FrontRadar,
    Scc,
    Corner235,
    Corner180,
    Corner430,
}

fn promoted<S: Serializer>(value: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    crate::scalar::serialize_float(&f64::from(*value), serializer)
}

fn rounded<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    Ok(crate::scalar::deserialize_float(deserializer)? as f32)
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Point {
    pub track_id: u64,
    #[serde(serialize_with = "promoted", deserialize_with = "rounded")]
    pub d_rel: f32,
    #[serde(serialize_with = "promoted", deserialize_with = "rounded")]
    pub y_rel: f32,
    #[serde(serialize_with = "promoted", deserialize_with = "rounded")]
    pub v_rel: f32,
    #[serde(serialize_with = "promoted", deserialize_with = "rounded")]
    pub a_rel: f32,
    #[serde(serialize_with = "promoted", deserialize_with = "rounded")]
    pub yv_rel: f32,
    pub measured: bool,
    #[serde(serialize_with = "promoted", deserialize_with = "rounded")]
    pub v_lead: f32,
    #[serde(serialize_with = "promoted", deserialize_with = "rounded")]
    pub a_lead: f32,
    #[serde(serialize_with = "promoted", deserialize_with = "rounded")]
    pub j_lead: f32,
    pub radar_source: Source,
    pub track_state: u8,
}
