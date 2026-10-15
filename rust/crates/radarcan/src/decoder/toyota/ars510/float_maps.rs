//! Trace serialization keeps nonfinite source floats without changing runtime storage.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Serialize, Deserialize)]
struct Float(
    #[serde(
        serialize_with = "crate::scalar::serialize_float",
        deserialize_with = "crate::scalar::deserialize_float"
    )]
    f64,
);
pub fn serialize<S: serde::Serializer>(
    values: &BTreeMap<u64, Vec<f64>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    values
        .iter()
        .map(|(id, values)| {
            (
                *id,
                values.iter().map(|value| Float(*value)).collect::<Vec<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>()
        .serialize(serializer)
}
pub fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<u64, Vec<f64>>, D::Error> {
    BTreeMap::<String, Vec<Float>>::deserialize(deserializer)?
        .into_iter()
        .map(|(id, values)| {
            let id = id.parse().map_err(serde::de::Error::custom)?;
            Ok((id, values.into_iter().map(|value| value.0).collect()))
        })
        .collect()
}
