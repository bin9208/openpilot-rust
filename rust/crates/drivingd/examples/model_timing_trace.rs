use openpilot_driving_modeld::diagnostics::{context, FrameTiming};
use openpilot_logging::{diagnostics::Diagnostics, Fields, Number, Value};
use serde::Deserialize;
use std::io::{self, BufRead};

#[derive(Deserialize)]
struct Step {
    frame_id: u32,
    loop_start: f64,
    cpu_start: f64,
    camera_ready: f64,
    camera_age_at_run_ms: f64,
    inference_seconds: f64,
    inference_cpu_ms: f64,
    inference_finished: f64,
    postprocess_end: f64,
    loop_end: f64,
    cpu_end: f64,
    dropped: u32,
    published: bool,
    now: f64,
    scheduler: Option<[u64; 3]>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut diagnostics =
        Diagnostics::new("modeld", 1.0, 10.0, Some([100, 200, 3]), Some(false), 123);
    for line in io::stdin().lock().lines() {
        let step: Step = serde_json::from_str(&line?)?;
        let timing = FrameTiming {
            frame_id: step.frame_id,
            loop_start: step.loop_start,
            cpu_start: step.cpu_start,
            camera_ready: step.camera_ready,
            camera_age_at_run_ms: step.camera_age_at_run_ms,
            inference_seconds: step.inference_seconds,
            inference_cpu_ms: step.inference_cpu_ms,
            inference_finished: step.inference_finished,
            postprocess_end: step.postprocess_end,
            loop_end: step.loop_end,
            cpu_end: step.cpu_end,
            dropped: step.dropped,
            published: step.published,
        };
        let metrics = timing.metrics();
        let raw: Fields = metrics
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    match value {
                        Number::Float(value) => Value::Float(*value),
                        Number::Integer(value) => Value::Integer(i128::from(*value)),
                    },
                )
            })
            .collect();
        let event = diagnostics.record_with(
            metrics,
            context(step.frame_id),
            || step.now,
            || step.scheduler,
        )?;
        let output: Fields = [
            ("values".into(), Value::Object(raw)),
            ("event".into(), event.map_or(Value::Null, Value::Object)),
        ]
        .into_iter()
        .collect();
        println!("{}", output.to_json()?);
    }
    Ok(())
}
