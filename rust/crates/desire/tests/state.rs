use openpilot_desire::{
    helper::DesireHelper,
    side::SideState,
    types::{Config, Input, Lead, State},
};

#[test]
fn trailer_blocks_both_sides_and_remote_commands() {
    let mut helper = DesireHelper::default();
    let mut input = Input::default();
    input.car.left_blinker = true;
    input.car.trailer_connected = true;
    let mut allowed = None;
    let desire = helper
        .update(&input, Config::default, |value| {
            allowed = Some(value);
            Some("laneLeft".to_owned())
        })
        .unwrap();
    assert_eq!(allowed, Some(false));
    assert_eq!(desire, 0);
    assert_eq!(helper.lane_change_state, State::Off);
    assert!(!helper.left.lane_change_available);
    assert!(!helper.right.lane_change_available);
}

#[test]
fn close_receding_corner_track_releases_hold_only_after_confirmation() {
    let mut side = SideState::new("left");
    let no_lead = Lead::default();
    side.update_obstacles(25.0, &no_lead, true, false, &[]);
    for frame in 0..6 {
        let lead = Lead {
            status: true,
            d_rel: 4.0 + f64::from(frame) * 0.45,
            v_rel: Some(9.0),
            radar_track_id: 1661,
            ..Lead::default()
        };
        side.update_obstacles(25.0, &no_lead, false, false, &[lead]);
        if frame < 5 {
            assert!(side.bsd_hold_counter > 0);
        }
    }
    assert_eq!(side.bsd_hold_counter, 0);
    assert!(!side.side_object_detected);
}

#[test]
fn invalid_geometry_does_not_advance_state() {
    let mut helper = DesireHelper::default();
    let mut input = Input::default();
    input.model.lane_lines[0].clear();
    assert!(helper.update(&input, Config::default, |_| None).is_err());
    assert_eq!(helper.frame, 0);
}

#[test]
fn constant_infinite_geometry_matches_numpy_interpolation() {
    let mut helper = DesireHelper::default();
    let mut input = Input::default();
    input.model.lane_lines[0] = vec![f64::INFINITY; 33];
    input.model.lane_lines[1] = vec![0.0; 33];
    input.model.road_edges[0] = vec![f64::INFINITY; 33];
    helper.update(&input, Config::default, |_| None).unwrap();
    assert_eq!(helper.left.lane_width, f64::INFINITY);
    assert_eq!(helper.left.dist_to_edge, f64::INFINITY);
    assert_eq!(helper.left.dist_to_edge_far, f64::INFINITY);
}
