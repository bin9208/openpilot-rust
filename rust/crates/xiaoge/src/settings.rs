use crate::{numbers, Error};
use num_traits::ToPrimitive;
use openpilot_logmessaged::JsonValue;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub threshold: f64,
    pub smoothing_seconds: f64,
    pub base_interval_seconds: f64,
    pub lane_threshold: f64,
    pub lane_interval_seconds: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            threshold: 0.45,
            smoothing_seconds: 0.2,
            base_interval_seconds: 0.25,
            lane_threshold: 0.25,
            lane_interval_seconds: 0.4,
        }
    }
}

impl Settings {
    pub fn patch(self, value: &Value) -> Result<Self, Error> {
        self.patch_json(&numbers::json(value)?)
    }

    pub fn patch_json(self, value: &JsonValue) -> Result<Self, Error> {
        if !value.is_object() {
            return Err(Error::Invalid("settings must be a JSON object"));
        }
        let field = |name, prior| {
            value
                .get(name)
                .map_or(Ok(Some(prior)), |value| numbers::python_number(&value))?
                .ok_or(Error::Invalid("settings must be numeric"))
        };
        let result = Self {
            threshold: field("threshold", self.threshold)?,
            smoothing_seconds: field("smoothingSeconds", self.smoothing_seconds)?,
            base_interval_seconds: field("baseIntervalSeconds", self.base_interval_seconds)?,
            lane_threshold: field("laneThreshold", self.lane_threshold)?,
            lane_interval_seconds: field("laneIntervalSeconds", self.lane_interval_seconds)?,
        };
        for (value, minimum, maximum, error) in [
            (
                result.threshold,
                0.25,
                1.0,
                "threshold must be 0.25 to 1.00",
            ),
            (
                result.smoothing_seconds,
                0.1,
                0.5,
                "smoothingSeconds must be 0.1 to 0.5",
            ),
            (
                result.base_interval_seconds,
                0.05,
                1.0,
                "baseIntervalSeconds must be 0.05 to 1.00",
            ),
            (
                result.lane_threshold,
                0.05,
                1.0,
                "laneThreshold must be 0.05 to 1.0",
            ),
            (
                result.lane_interval_seconds,
                0.05,
                2.0,
                "laneIntervalSeconds must be 0.05 to 2.0",
            ),
        ] {
            if !(minimum..=maximum).contains(&value) {
                return Err(Error::Invalid(error));
            }
        }
        Ok(result)
    }

    pub fn from_parameters(mut read: impl FnMut(&str) -> Option<Vec<u8>>) -> Self {
        let mut integer = |name, default, minimum, maximum, scale| {
            let value = read(name)
                .filter(|bytes| bytes.is_ascii())
                .and_then(|bytes| {
                    std::str::from_utf8(&bytes)
                        .ok()
                        .and_then(numbers::integer_text)
                })
                .unwrap_or(default);
            value.clamp(minimum, maximum) / scale
        };
        Self {
            threshold: integer("OnnxBsdThreshold", 45.0, 25.0, 100.0, 100.0),
            smoothing_seconds: integer("OnnxBsdSmoothingMs", 200.0, 100.0, 500.0, 1000.0),
            base_interval_seconds: integer("OnnxBsdIntervalMs", 250.0, 50.0, 1000.0, 1000.0),
            lane_threshold: integer("OnnxLaneThreshold", 25.0, 5.0, 100.0, 100.0),
            lane_interval_seconds: integer("OnnxLaneIntervalMs", 400.0, 50.0, 2000.0, 1000.0),
        }
    }

    pub fn parameters(self) -> Result<Vec<(&'static str, i32)>, Error> {
        [
            ("OnnxBsdThreshold", self.threshold * 100.0),
            ("OnnxBsdSmoothingMs", self.smoothing_seconds * 1000.0),
            ("OnnxBsdIntervalMs", self.base_interval_seconds * 1000.0),
            ("OnnxLaneThreshold", self.lane_threshold * 100.0),
            ("OnnxLaneIntervalMs", self.lane_interval_seconds * 1000.0),
        ]
        .into_iter()
        .map(|(name, value)| {
            value
                .round_ties_even()
                .to_i32()
                .map(|value| (name, value))
                .ok_or(Error::Invalid("setting outside integer parameter range"))
        })
        .collect()
    }
}
