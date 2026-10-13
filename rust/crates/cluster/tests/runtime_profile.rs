use openpilot_cluster::runtime::profile::{Reporter, SampleStats};

#[test]
fn source_stats_and_stable_average_order_are_preserved() {
    let mut stats = SampleStats::default();
    for sample in [-3.0, 0.0, 6.0] {
        stats.add(sample);
    }
    assert_eq!(stats.count, 3);
    assert_eq!(stats.average().to_bits(), 1.0_f64.to_bits());
    assert_eq!(stats.max_ms.to_bits(), 6.0_f64.to_bits());
    assert_eq!(stats.last_ms.to_bits(), 6.0_f64.to_bits());
    let mut reporter = Reporter::new(true, 0.01, 10.0);
    reporter.add("first", 1.0);
    reporter.add("second", 1.0);
    reporter.add("slow", 2.0);
    reporter.frame_done();
    assert!(reporter.report(10.1, "owned").is_none());
    let report = reporter.report(10.3, "owned").unwrap();
    assert!(report.starts_with("PROFILE 1 frames / 0.3s\n  runtime owned\n"));
    assert!(report.find("slow").unwrap() < report.find("first").unwrap());
    assert!(report.find("first").unwrap() < report.find("second").unwrap());
    assert_eq!(
        reporter.report(11.0, "").unwrap(),
        "PROFILE 0 frames / 0.7s\n"
    );
}

#[test]
fn disabled_reporter_keeps_no_samples_or_frames() {
    let mut reporter = Reporter::new(false, 0.2, 0.0);
    reporter.add("unused", 20.0);
    reporter.frame_done();
    assert!(reporter.report(2.0, "ignored").is_none());
}
