//! Original modeld phase definitions, fed only by the Rust loop's actual clocks and results.
use openpilot_logging::{
    log_site, producer::Logger, runtime::RuntimeDiagnostics, Fields, Number, Value,
};

pub fn thread_cpu() -> f64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    now.tv_sec as f64 + now.tv_nsec as f64 * 1e-9
}

pub struct FrameTiming {
    pub frame_id: u32,
    pub loop_start: f64,
    pub cpu_start: f64,
    pub camera_ready: f64,
    pub camera_age_at_run_ms: f64,
    pub inference_seconds: f64,
    pub inference_cpu_ms: f64,
    pub inference_finished: f64,
    pub postprocess_end: f64,
    pub loop_end: f64,
    pub cpu_end: f64,
    pub dropped: u32,
    pub published: bool,
}
impl FrameTiming {
    pub fn metrics(&self) -> [(String, Number); 9] {
        [
            (
                "camera_wait_ms",
                Number::Float((self.camera_ready - self.loop_start) * 1000.0),
            ),
            (
                "camera_age_at_run_ms",
                Number::Float(self.camera_age_at_run_ms),
            ),
            (
                "inference_ms",
                Number::Float(self.inference_seconds * 1000.0),
            ),
            (
                "inference_thread_cpu_ms",
                Number::Float(self.inference_cpu_ms),
            ),
            (
                "postprocess_ms",
                Number::Float((self.postprocess_end - self.inference_finished) * 1000.0),
            ),
            (
                "loop_ms",
                Number::Float((self.loop_end - self.loop_start) * 1000.0),
            ),
            (
                "thread_cpu_ms",
                Number::Float((self.cpu_end - self.cpu_start) * 1000.0),
            ),
            ("dropped_frames", Number::Integer(i64::from(self.dropped))),
            ("published", Number::Integer(i64::from(self.published))),
        ]
        .map(|(name, value)| (name.into(), value))
    }

    pub fn record(&self, diagnostics: &mut RuntimeDiagnostics, logger: &mut Logger) {
        // Observability must not interrupt inference. RuntimeDiagnostics resets its aggregate
        // before emission and suppresses the sink failure, matching the original source.
        let _ = diagnostics.record(logger, log_site!(), self.metrics(), context(self.frame_id));
    }
}

pub fn context(frame_id: u32) -> Fields {
    [
        (
            "backend".into(),
            Value::Text(std::any::type_name::<crate::runtime::DrivingRuntime<'_>>().into()),
        ),
        // This daemon loads only the native internal model; Jetlink keeps that model warm.
        ("usbgpu".into(), Value::Bool(false)),
        ("frame_id".into(), Value::Integer(i128::from(frame_id))),
    ]
    .into_iter()
    .collect()
}
