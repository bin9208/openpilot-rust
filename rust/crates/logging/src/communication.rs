use crate::{runtime::monotonic, value::round3, Error, Fields, Value};
use openpilot_messaging::{frequency::MovingAverage, state::State};

pub fn snapshot(state: &State, services: &[&str]) -> Result<Fields, Error> {
    snapshot_at(state, services, monotonic())
}

pub fn snapshot_at(state: &State, services: &[&str], now: f64) -> Result<Fields, Error> {
    let mut result = Fields::new();
    for name in services {
        let Ok(topic) = state.topic(name) else {
            continue;
        };
        let tracker = &topic.tracker;
        let fields = [
            ("avg_hz", hz(&tracker.average)?),
            ("recent_hz", hz(&tracker.recent)?),
            ("min_hz", Value::Float(tracker.min_frequency)),
            ("max_hz", Value::Float(tracker.max_frequency)),
            (
                "recv_age_ms",
                if topic.seen {
                    Value::Float(round3((now - topic.receive_time) * 1000.0)?)
                } else {
                    Value::Null
                },
            ),
            ("valid", Value::Bool(topic.valid)),
            ("alive", Value::Bool(topic.alive)),
            ("freq_ok", Value::Bool(topic.frequency_ok)),
            ("ignore_alive", Value::Bool(topic.ignores_alive())),
            ("ignore_valid", Value::Bool(topic.ignores_valid())),
            ("ignore_freq", Value::Bool(topic.ignores_frequency())),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect();
        result.insert((*name).into(), Value::Object(fields));
    }
    Ok(result)
}

fn hz(average: &MovingAverage) -> Result<Value, Error> {
    let dt = if average.count == 0 {
        0.0
    } else {
        average.average()
    };
    Ok(if dt > 0.0 {
        Value::Float(round3(1.0 / dt)?)
    } else {
        Value::Null
    })
}
