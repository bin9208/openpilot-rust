use openpilot_cluster::route_replay::{timeline::Timeline, Frame};

fn frame(time: f64, speed: f64) -> Frame {
    let mut frame = Frame {
        t: time,
        ..Frame::default()
    };
    frame.state.speed_kph = speed;
    frame
}
#[test]
fn duplicate_time_playback_and_status_keep_source_selection_and_trim_boundaries() {
    let mut timeline = Timeline::default();
    timeline.append(
        0,
        "route--0/rlog.zst".into(),
        vec![frame(0.0, 10.0), frame(0.0, 20.0), frame(10.0, 30.0)],
    );
    assert_eq!(
        timeline
            .playback_frame(0.0)
            .unwrap()
            .state
            .speed_kph
            .to_bits(),
        20.0_f64.to_bits()
    );
    assert_eq!(
        timeline
            .status_frame(0.0)
            .unwrap()
            .state
            .speed_kph
            .to_bits(),
        10.0_f64.to_bits()
    );
    timeline.append(
        1,
        "route--1/rlog.zst".into(),
        vec![frame(10.0, 40.0), frame(20.0, 50.0)],
    );
    timeline.append(
        2,
        "route--2/rlog.zst".into(),
        vec![frame(20.0, 60.0), frame(30.0, 70.0)],
    );
    timeline.trim(11.0);
    assert_eq!(timeline.chunk_count(), 3);
    timeline.trim(11.001);
    assert_eq!(timeline.chunk_count(), 2);
    assert_eq!(timeline.first_time().unwrap().to_bits(), 10.0_f64.to_bits());
    assert_eq!(timeline.duration().to_bits(), 30.0_f64.to_bits());
    assert_eq!(
        timeline.log_path(20.4).unwrap().to_string_lossy(),
        "route--1/rlog.zst"
    );
    assert_eq!(timeline.log_offset(21.0).to_bits(), 1.0_f64.to_bits());
    assert_eq!(
        timeline
            .playback_frame(15.0)
            .unwrap()
            .state
            .speed_kph
            .to_bits(),
        45.0_f64.to_bits()
    );
}
