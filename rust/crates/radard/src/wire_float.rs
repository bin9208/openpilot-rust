use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod scalar {
    pub use openpilot_radarcan::scalar::{
        deserialize_float as deserialize, serialize_float as serialize,
    };
}

pub mod optional {
    use super::{Deserialize, Deserializer, Number, Serialize, Serializer};

    pub fn serialize<S: Serializer>(
        values: &Option<Vec<f64>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        values
            .as_ref()
            .map(|values| {
                values
                    .iter()
                    .map(|value| Number(*value))
                    .collect::<Vec<_>>()
            })
            .serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Vec<f64>>, D::Error> {
        Ok(Option::<Vec<Number>>::deserialize(deserializer)?
            .map(|values| values.into_iter().map(|number| number.0).collect()))
    }
}

#[derive(Deserialize, Serialize)]
struct Number(
    #[serde(
        serialize_with = "openpilot_radarcan::scalar::serialize_float",
        deserialize_with = "openpilot_radarcan::scalar::deserialize_float"
    )]
    f64,
);

pub fn serialize<S: Serializer>(values: &[f64], serializer: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut sequence = serializer.serialize_seq(Some(values.len()))?;
    for value in values {
        sequence.serialize_element(&Number(*value))?;
    }
    sequence.end()
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<f64>, D::Error> {
    Ok(Vec::<Number>::deserialize(deserializer)?
        .into_iter()
        .map(|number| number.0)
        .collect())
}
