//! carrot's modeld/camera_gap.py: bounded evidence for one camera gap. It
//! never changes camera selection or pose validity.
use openpilot_logging::{log_site, producer::Logger, record::Record, Fields, Value};
use openpilot_modeld::camera::Received;

#[derive(Default)]
pub struct CameraGapTrace {
    /// Receive calls of the current pairing attempt.
    pub received: Vec<Received>,
    previous: Option<Value>,
    next_emit: f64,
}

/// Python `round(value, 3)` for the recorded milliseconds.
fn rounded(value: f64) -> Value {
    Value::Float((value * 1000.0).round_ties_even() / 1000.0)
}

fn optional(value: Option<u64>) -> Value {
    value.map_or(Value::Null, |value| Value::Integer(i128::from(value)))
}

impl CameraGapTrace {
    fn current(&self, frame_id: u32, backend: &str, timings_ms: &[(&str, f64)]) -> Value {
        let received = self
            .received
            .iter()
            .map(|entry| {
                Value::Object(
                    [
                        ("camera".to_owned(), Value::Text(entry.camera.to_owned())),
                        (
                            "frame_id".to_owned(),
                            optional(entry.frame_id.map(u64::from)),
                        ),
                        ("sof".to_owned(), optional(entry.sof)),
                        ("wait_ms".to_owned(), rounded(entry.wait_ms)),
                    ]
                    .into_iter()
                    .collect(),
                )
            })
            .collect();
        let timings: Fields = timings_ms
            .iter()
            .map(|(name, value)| ((*name).to_owned(), rounded(*value)))
            .collect();
        Value::Object(
            [
                ("frame_id".to_owned(), Value::Integer(i128::from(frame_id))),
                ("backend".to_owned(), Value::Text(backend.to_owned())),
                ("received".to_owned(), Value::Array(received)),
                ("timings_ms".to_owned(), Value::Object(timings)),
            ]
            .into_iter()
            .collect(),
        )
    }

    /// The event for this frame when frames were dropped and none was emitted
    /// within the last second; the frame is remembered as `previous` either way.
    pub fn fields(
        &mut self,
        now: f64,
        frame_id: u32,
        dropped: u32,
        backend: &str,
        timings_ms: &[(&str, f64)],
    ) -> Option<Fields> {
        let current = self.current(frame_id, backend, timings_ms);
        let event = (dropped > 0 && now >= self.next_emit).then(|| {
            self.next_emit = now + 1.0;
            [
                ("mono_time".to_owned(), Value::Float(now)),
                ("dropped".to_owned(), Value::Integer(i128::from(dropped))),
                (
                    "previous".to_owned(),
                    self.previous.clone().unwrap_or(Value::Null),
                ),
                ("current".to_owned(), current.clone()),
            ]
            .into_iter()
            .collect()
        });
        self.previous = Some(current);
        event
    }

    pub fn record(
        &mut self,
        logger: &mut Logger,
        now: f64,
        frame_id: u32,
        dropped: u32,
        backend: &str,
        timings_ms: &[(&str, f64)],
    ) {
        if let Some(fields) = self.fields(now, frame_id, dropped, backend, timings_ms) {
            // Evidence only: a logging failure must not interrupt inference.
            if let Ok(record) = Record::event("modelCameraGap", Vec::new(), fields) {
                let _ = logger.emit(log_site!(), record);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace() -> CameraGapTrace {
        CameraGapTrace {
            received: vec![
                Received {
                    camera: "main",
                    frame_id: Some(41),
                    sof: Some(2_050_000_000),
                    wait_ms: 61.23456,
                },
                Received {
                    camera: "extra",
                    frame_id: None,
                    sof: None,
                    wait_ms: 100.0,
                },
            ],
            ..CameraGapTrace::default()
        }
    }
    const TIMINGS: [(&str, f64); 2] = [("camera_wait", 61.2349), ("loop", 63.0)];

    #[test]
    fn a_gap_is_recorded_with_the_frame_before_it_at_most_once_a_second() {
        let mut gap = trace();
        // No drop: nothing is emitted, the frame becomes `previous`.
        assert!(gap.fields(10.0, 40, 0, "internal", &TIMINGS).is_none());
        let event = gap.fields(10.05, 41, 1, "jetlink", &TIMINGS).unwrap();
        assert_eq!(event["dropped"], Value::Integer(1));
        assert_eq!(event["mono_time"], Value::Float(10.05));
        let Value::Object(previous) = &event["previous"] else {
            panic!("previous frame missing");
        };
        assert_eq!(previous["frame_id"], Value::Integer(40));
        let Value::Object(current) = &event["current"] else {
            panic!("current frame missing");
        };
        assert_eq!(current["backend"], Value::Text("jetlink".into()));
        let Value::Object(timings) = &current["timings_ms"] else {
            panic!("timings missing");
        };
        assert_eq!(timings["camera_wait"], Value::Float(61.235));
        let Value::Array(received) = &current["received"] else {
            panic!("receive trace missing");
        };
        let Value::Object(extra) = &received[1] else {
            panic!("receive entry missing");
        };
        assert_eq!(extra["frame_id"], Value::Null);
        assert_eq!(extra["sof"], Value::Null);
        // A second gap within the same second is not emitted.
        assert!(gap.fields(10.5, 43, 1, "jetlink", &TIMINGS).is_none());
        assert!(gap.fields(11.06, 45, 2, "jetlink", &TIMINGS).is_some());
    }
}
