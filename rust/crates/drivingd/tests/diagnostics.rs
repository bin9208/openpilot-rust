use openpilot_driving_modeld::diagnostics::{context, thread_cpu, FrameTiming};
use openpilot_logging::{Fields, Number, Value};

#[test]
fn frame_timing_keeps_source_phase_order_and_integer_counts() {
    let timing = FrameTiming {
        frame_id: 51,
        loop_start: 10.0,
        cpu_start: 1.0,
        camera_ready: 10.1,
        camera_age_at_run_ms: 21.0,
        inference_seconds: 0.05,
        inference_cpu_ms: 19.0,
        inference_finished: 10.3,
        postprocess_end: 10.35,
        loop_end: 10.36,
        cpu_end: 1.05,
        dropped: 2,
        published: false,
    };
    let metrics = timing.metrics();
    let names: Vec<_> = metrics.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "camera_wait_ms",
            "camera_age_at_run_ms",
            "inference_ms",
            "inference_thread_cpu_ms",
            "postprocess_ms",
            "loop_ms",
            "thread_cpu_ms",
            "dropped_frames",
            "published"
        ]
    );
    assert!(matches!(metrics[7].1, Number::Integer(2)));
    assert!(matches!(metrics[8].1, Number::Integer(0)));
    let fields: Fields = context(51);
    assert_eq!(fields["frame_id"], Value::Integer(51));
    assert_eq!(fields["usbgpu"], Value::Bool(false));
    assert_eq!(
        fields["backend"],
        Value::Text("openpilot_driving_modeld::runtime::DrivingRuntime<'_>".into())
    );
}

#[test]
fn cpu_clock_measures_the_calling_thread() {
    let start = thread_cpu();
    let worker = std::thread::spawn(|| {
        let target = thread_cpu() + 0.03;
        while thread_cpu() < target {
            std::hint::black_box(13_u64.wrapping_mul(19));
        }
    });
    std::thread::sleep(std::time::Duration::from_millis(50));
    worker.join().unwrap();
    let after_sleep = thread_cpu();
    assert!(after_sleep >= start && after_sleep - start < 0.02);
    let target = after_sleep + 0.003;
    while thread_cpu() < target {
        std::hint::black_box(11_u64.wrapping_mul(17));
    }
    assert!(thread_cpu() >= target);
}
