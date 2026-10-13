use openpilot_cluster::route_replay::Frame;

#[test]
fn replay_blends_continuous_state_but_keeps_recorded_lane_change_and_radar_discrete() {
    let mut left = Frame {
        t: 1.0,
        ..Default::default()
    };
    left.state.speed_kph = 20.0;
    left.state.lane_change = Some("left".into());
    left.state.lane_change_phase = "changing".into();
    left.state.lane_change_progress = 0.2;
    left.state.display_speed_kph = None;
    left.state
        .radar_points
        .push(openpilot_cluster::models::RadarPoint {
            label: "recorded-left".into(),
            longitudinal_m: 20.0,
            ..Default::default()
        });
    let mut right = left.clone();
    right.t = 2.0;
    right.state.speed_kph = 40.0;
    right.state.display_speed_kph = Some(42.0);
    right.state.lane_change_progress = 0.9;
    right.state.radar_points[0].label = "recorded-right".into();
    right.state.radar_points[0].longitudinal_m = 40.0;
    let quarter = left.blend(&right, 0.25);
    assert_eq!(quarter.state.speed_kph.to_bits(), 25.0_f64.to_bits());
    assert_eq!(
        quarter.state.display_speed_kph.unwrap().to_bits(),
        42.0_f64.to_bits()
    );
    assert_eq!(
        quarter.state.lane_change_progress.to_bits(),
        0.2_f64.to_bits()
    );
    let state = quarter.to_state();
    assert_eq!(state.radar_points, left.state.radar_points);
    assert_eq!(state.ego_lane_offset.to_bits(), (-0.2_f64).to_bits());
    assert_eq!(
        state.highlight_lane_offset.unwrap().to_bits(),
        (-1.0_f64).to_bits()
    );
    let half = left.blend(&right, 0.5);
    assert_eq!(half.state.radar_points, right.state.radar_points);
    assert_eq!(half.state.lane_change_progress.to_bits(), 0.9_f64.to_bits());
}
