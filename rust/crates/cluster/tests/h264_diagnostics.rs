use openpilot_cluster::h264::diagnostics::{nal_stats, Diagnostics};

#[test]
fn interval_and_forced_reports_reset_the_window() {
    let mut diagnostic = Diagnostics::new(2.0, 10.0);
    diagnostic.record_unit("native", &[0, 0, 1, 0x65, 9, 0], &[6], false);
    diagnostic.record_queue_depth(3);
    diagnostic.record_drop(1, 2);
    diagnostic.record_send("native", 6, 2.5);
    assert!(diagnostic.report(11.0, "native", false).is_none());
    let report = diagnostic.report(12.0, "native", false).unwrap();
    assert!(report.contains("units=1 key=1"));
    assert!(report.contains("max_nal=2 au_qmax=3 drop_units=1 drop_chunks=2"));
    assert!(report.ends_with("send_ms_avg=2.50 max=2.50"));
    assert!(diagnostic.report(20.0, "native", true).is_none());
    diagnostic.record_send("", 100, 1.0);
    assert!(diagnostic
        .report(20.0, "ffmpeg", true)
        .unwrap()
        .contains("backend=ffmpeg source=ffmpeg"));
}

#[test]
fn disabled_diagnostics_remain_empty() {
    let mut diagnostic = Diagnostics::new(0.0, 1.0);
    diagnostic.record_unit("native", &[0, 0, 1, 0x65], &[4], true);
    diagnostic.record_send("native", 4, 1.0);
    assert!(diagnostic.report(100.0, "native", true).is_none());
}

#[test]
fn nal_stats_handle_empty_units_padding_and_mixed_start_codes() {
    assert_eq!(nal_stats(&[]), (0, 0, false));
    assert_eq!(nal_stats(&[0, 0, 0, 1, 0, 0, 1]), (0, 0, false));
    assert_eq!(
        nal_stats(&[0, 0, 0, 1, 0x67, 9, 0, 0, 1, 0x65, 3, 4, 0, 0]),
        (2, 3, true)
    );
}
