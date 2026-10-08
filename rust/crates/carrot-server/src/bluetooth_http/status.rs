use super::{bad, Failure, Service, Value};
use crate::json_fields::set;

pub(super) async fn status(service: &Service) -> Result<Value, Failure> {
    let actions = [
        "none",
        "accelCruise",
        "decelCruise",
        "gapAdjustCruise",
        "lfaButton",
        "cancel",
        "accelCruiseLong",
        "decelCruiseLong",
        "gapAdjustCruiseLong",
        "lfaButtonLong",
        "cancelLong",
        "laneLeft",
        "laneRight",
        "paddleDecel",
        "carrotCruise",
    ];
    let defaults = [
        ("up", "accelCruise"),
        ("down", "decelCruise"),
        ("left", "laneLeft"),
        ("right", "laneRight"),
        ("center", "paddleDecel"),
        ("1", "gapAdjustCruise"),
        ("2", "none"),
    ];
    let mut result = Value::object([
        ("runtime", service.runtime()?),
        ("config", super::value(&service.settings())?),
        (
            "actions",
            Value::Array(actions.into_iter().map(Value::text).collect()),
        ),
        (
            "defaults",
            Value::Object(
                defaults
                    .into_iter()
                    .map(|(key, value)| (key.chars().map(u32::from).collect(), Value::text(value)))
                    .collect(),
            ),
        ),
        ("radioEnabled", Value::Bool(service.radio_enabled().await?)),
    ]);
    match service.snapshot().await {
        Ok(snapshot) => {
            if let Value::Object(fields) = super::value(&snapshot)? {
                for (key, value) in fields {
                    let Value::Object(result_fields) = &mut result else {
                        return Err(bad("status object required"));
                    };
                    crate::json_fields::insert(result_fields, key, value);
                }
            }
            set(&mut result, "available", Value::Bool(true))?;
        }
        Err(error) => {
            set(&mut result, "available", Value::Bool(false))?;
            set(&mut result, "error", Value::text(&error.to_string()))?;
            set(&mut result, "devices", Value::Array(Vec::new()))?;
            set(&mut result, "adapters", Value::Array(Vec::new()))?;
        }
    }
    Ok(result)
}
