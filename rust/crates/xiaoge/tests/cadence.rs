use openpilot_xiaoge::service::state::{Metrics, Timing};

#[test]
fn fast_inference_preserves_the_configured_start_cadence() {
    let mut metrics = Metrics::new(1.0);
    assert!(metrics.ready(1.0, 0.4));
    metrics
        .record(Timing {
            received: 1.0,
            started: 1.0,
            finished: 1.1,
            thread_cpu_ms: 40.0,
        })
        .unwrap();
    assert!(!metrics.ready(1.39, 0.4));
    assert!(metrics.ready(1.41, 0.4));
    assert_eq!(metrics.last_inference, 1.0);
}

#[test]
fn overloaded_inference_leaves_one_interval_after_completion() {
    let mut metrics = Metrics::new(1.0);
    metrics
        .record(Timing {
            received: 1.0,
            started: 1.01,
            finished: 2.01,
            thread_cpu_ms: 800.0,
        })
        .unwrap();
    assert!(!metrics.ready(2.02, 0.4));
    assert!(!metrics.ready(2.40, 0.4));
    assert!(metrics.ready(2.42, 0.4));
    assert_eq!(metrics.last_inference, 1.0);
    assert_eq!(metrics.count, 1);
}

#[test]
fn cooldown_uses_current_bsd_followup_interval_and_recovers() {
    let mut metrics = Metrics::new(1.0);
    metrics
        .record(Timing {
            received: 1.0,
            started: 1.0,
            finished: 1.5,
            thread_cpu_ms: 300.0,
        })
        .unwrap();
    assert!(!metrics.ready(1.64, 0.15));
    assert!(metrics.ready(1.66, 0.15));
    assert!(!metrics.ready(1.74, 0.25));
    assert!(metrics.ready(1.76, 0.25));
    assert!(!metrics.ready(1.9, 1.0));
    assert!(metrics.ready(2.01, 1.0));
    metrics
        .record(Timing {
            received: 3.0,
            started: 3.0,
            finished: 3.1,
            thread_cpu_ms: 60.0,
        })
        .unwrap();
    assert!(metrics.ready(3.41, 0.4));
    assert!(!metrics.ready(3.39, 0.4));
}
