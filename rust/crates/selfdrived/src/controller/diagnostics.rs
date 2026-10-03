use super::Error;
use openpilot_logging::{Fields, PythonText, Value};
use openpilot_logmessaged::JsonValue;
use openpilot_messaging::state::State;
use serde::Serialize;
use std::collections::BTreeSet;

pub(super) fn process_names(names: &BTreeSet<String>) -> Result<Value, Error> {
    let array = JsonValue::parse(&serde_json::to_string(names)?)?;
    let mut points = openpilot_runtime_version::python_str(&array)?;
    if let Some(first) = points.first_mut() {
        *first = u32::from('{');
    }
    if let Some(last) = points.last_mut() {
        *last = u32::from('}');
    }
    Ok(Value::PythonText(PythonText::new(points)?))
}

#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct Issues {
    pub invalid: Vec<String>,
    pub not_alive: Vec<String>,
    pub not_freq_ok: Vec<String>,
}
impl Issues {
    pub fn fields(&self) -> Fields {
        [
            ("invalid", &self.invalid),
            ("not_alive", &self.not_alive),
            ("not_freq_ok", &self.not_freq_ok),
        ]
        .into_iter()
        .map(|(key, names)| {
            (
                key.into(),
                Value::Array(names.iter().cloned().map(Value::Text).collect()),
            )
        })
        .collect()
    }
    pub fn services(&self) -> Vec<&str> {
        let mut names = Vec::new();
        for name in self
            .not_freq_ok
            .iter()
            .chain(&self.not_alive)
            .chain(&self.invalid)
            .map(String::as_str)
            .chain(["modelV2", "driverAssistance", "longitudinalPlan"])
        {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }
}
pub fn issues(state: &State) -> Issues {
    let names = |predicate: fn(&openpilot_messaging::state::Topic) -> bool| {
        state
            .topics()
            .iter()
            .filter(|topic| predicate(topic))
            .map(|topic| topic.service.name.to_owned())
            .collect()
    };
    Issues {
        invalid: names(|topic| !topic.valid),
        not_alive: names(|topic| !topic.alive),
        not_freq_ok: names(|topic| !topic.frequency_ok),
    }
}
fn rounded(value: f64) -> Result<Value, Error> {
    Ok(openpilot_logging::Number::Float(value).rounded()?)
}
pub fn communication(state: &State, names: &[&str], now: f64) -> Result<Value, Error> {
    let mut values = Fields::new();
    for name in names {
        let topic = state.topic(name)?;
        let hz = |average: &openpilot_messaging::frequency::MovingAverage| {
            let dt = if average.count > 0 {
                average.average()
            } else {
                0.0
            };
            if dt > 0.0 {
                rounded(1.0 / dt)
            } else {
                Ok(Value::Null)
            }
        };
        let mut fields = Fields::new();
        fields.insert("avg_hz".into(), hz(&topic.tracker.average)?);
        fields.insert("recent_hz".into(), hz(&topic.tracker.recent)?);
        fields.insert("min_hz".into(), Value::Float(topic.tracker.min_frequency));
        fields.insert("max_hz".into(), Value::Float(topic.tracker.max_frequency));
        fields.insert(
            "recv_age_ms".into(),
            if topic.seen {
                rounded((now - topic.receive_time) * 1000.0)?
            } else {
                Value::Null
            },
        );
        for (key, value) in [
            ("valid", topic.valid),
            ("alive", topic.alive),
            ("freq_ok", topic.frequency_ok),
            ("ignore_alive", topic.ignores_alive()),
            ("ignore_valid", topic.ignores_valid()),
            ("ignore_freq", topic.ignores_frequency()),
        ] {
            fields.insert(key.into(), Value::Bool(value));
        }
        values.insert((*name).into(), Value::Object(fields));
    }
    Ok(Value::Object(values))
}
