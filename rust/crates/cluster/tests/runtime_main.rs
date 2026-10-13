use openpilot_cluster::runtime::Timing;
#[test]
fn elapsed_duration_and_source_fps_smoothing_retain_long_gap_policy() {
    let mut timing = Timing::new(10.0);
    assert_eq!(timing.step(10.1).to_bits(), (10.1_f64 - 10.0).to_bits());
    let first = timing.actual_fps.unwrap();
    timing.step(10.3);
    assert!(
        (timing.actual_fps.unwrap() - (first * 0.85 + 1.0 / (10.3_f64 - 10.1) * 0.15)).abs()
            < 1e-12
    );
    let prior = timing.actual_fps;
    timing.step(12.3);
    assert_eq!(timing.actual_fps, prior);
    assert!(timing.finished(12.3, Some(2.0)));
    assert!(!timing.finished(12.3, None));
}
