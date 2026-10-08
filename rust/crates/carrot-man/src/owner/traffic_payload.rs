use super::packet::{float, integer, py_text, truth};
use crate::navigation::Traffic;
use serde_json::Value;

pub fn traffic(value: &Value, detailed: bool) -> Option<Traffic> {
    if !value.is_object() {
        return None;
    }
    let mut signal = None;
    if detailed {
        for (name, lamp, remain) in [
            ("left", "left", "left_remain_time"),
            ("straight", "green", "straight_remain_time"),
            ("right", "right", "right_remain_time"),
            ("uturn", "uturn", "uturn_remain_time"),
        ] {
            if value
                .get(name)
                .is_some_and(|v| py_text(v).to_uppercase() == "GREEN_LIGHT_ON")
            {
                signal = Some((lamp, value.get(remain).and_then(float).unwrap_or(0.)));
                break;
            }
        }
        if signal.is_none() {
            let remain = ["straight", "left", "right", "uturn"]
                .into_iter()
                .filter(|name| {
                    value
                        .get(*name)
                        .is_some_and(|v| py_text(v).to_uppercase() == "RED_LIGHT_ON")
                })
                .filter_map(|name| value.get(format!("{name}_remain_time")).and_then(integer))
                .max();
            if let Some(remain) = remain {
                signal = Some((
                    "red",
                    num_traits::ToPrimitive::to_f64(&remain).unwrap_or(0.),
                ));
            }
        }
    } else {
        for (on, lamp, remain) in [
            ("redLightOn", "red", "redLightRemainTime"),
            ("leftLightOn", "left", "leftLightRemainTime"),
            ("greenLightOn", "green", "greenLightRemainTime"),
            ("rightLightOn", "right", "rightLightRemainTime"),
            ("uturnLightOn", "uturn", "uturnLightRemainTime"),
        ] {
            if value.get(on).is_some_and(truth) {
                signal = Some((lamp, value.get(remain).and_then(float).unwrap_or(0.)));
                break;
            }
        }
    }
    let (lamp, remain) = signal?;
    let remain = num_traits::ToPrimitive::to_i64(&remain.trunc())?;
    let distance = value.get("distance").and_then(float).unwrap_or(0.);
    if remain <= 0 || !distance.is_finite() {
        return None;
    }
    Some(Traffic {
        present: true,
        visible: true,
        distance,
        source: if detailed { "ssinf" } else { "sinf" }.into(),
        lamp: lamp.into(),
        remain,
    })
}
