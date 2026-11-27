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

fn drive(helper: &mut DesireHelper, input: &Input, frames: usize) -> Vec<(State, u8)> {
    (0..frames)
        .map(|_| {
            let desire = helper.update(input, Config::default, |_| None).unwrap();
            (helper.lane_change_state, desire)
        })
        .collect()
}

fn atc_input(kind: &str, kph: f64) -> Input {
    let mut input = Input::default();
    input.navigation.atc_type = kind.to_owned();
    input.car.v_ego = kph / 3.6;
    input
}

#[test]
fn atc_lane_change_continues_below_30_kph() {
    for (kind, desire) in [("atc right", 4), ("atc left", 3)] {
        let mut helper = DesireHelper::default();
        let trace = drive(&mut helper, &atc_input(kind, 20.0), 100);
        assert!(
            trace.contains(&(State::Starting, desire)),
            "{kind}: no automatic lane change at 20 km/h"
        );
    }
}

#[test]
fn atc_lane_change_stops_below_10_kph() {
    let mut helper = DesireHelper::default();
    let trace = drive(&mut helper, &atc_input("atc right", 8.0), 100);
    assert!(trace.iter().all(|(state, _)| *state == State::Off));
}

#[test]
fn driver_blinker_keeps_30_kph_floor() {
    let mut helper = DesireHelper::default();
    let mut input = atc_input("", 20.0);
    input.car.right_blinker = true;
    let trace = drive(&mut helper, &input, 100);
    assert!(trace.iter().all(|(state, _)| *state == State::Off));
}

#[test]
fn turn_without_pocket_gives_turn_desire() {
    let mut helper = DesireHelper::default();
    let trace = drive(&mut helper, &atc_input("turn right", 20.0), 100);
    assert!(trace.iter().all(|(state, _)| *state == State::Off));
    assert_eq!(trace.last().unwrap().1, 2);
    assert!(!helper.pocket_done);
}

#[test]
fn turn_enters_pocket_lane_once_before_turning() {
    let mut helper = DesireHelper::default();
    let mut input = atc_input("turn right", 20.0);
    // 오른쪽 차로 끝에 새 차로(포켓)가 붙어 있음: 가장자리까지 3.6 m, 그 사이 차선 확실
    input.model.road_edges[1] = vec![5.4; 33];
    let trace = drive(&mut helper, &input, 100);
    assert!(
        trace.contains(&(State::Starting, 4)),
        "no pocket lane change"
    );
    assert!(helper.pocket_done);
    // 옮긴 뒤(차선 변경 끝) 교차로에서 가장자리가 열리면 다시 포켓 진입을 하지 않고 회전 desire 로 넘어감
    input.lane_change_prob = 0.0;
    input.model.road_edges[1] = vec![7.2; 33];
    let trace = drive(&mut helper, &input, 200);
    assert_eq!(trace.last().unwrap().1, 2, "no turn desire after pocket");
}
