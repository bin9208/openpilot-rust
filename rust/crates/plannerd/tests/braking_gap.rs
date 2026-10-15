use openpilot_plannerd::braking_gap::{has_reserve, ComfortInput, LeadComfortState, Observation};

#[test]
fn stable_braking_lead_relief_ramps_then_withdraws_immediately() {
    let mut state = LeadComfortState::default();
    for _ in 0..20 {
        state.update(Observation {
            enabled: true,
            track_id: 1,
            dt: 0.05,
            distance: 60.,
            relative_speed: -1.,
        });
    }
    let speeds = [20.; 3];
    let gaps = [60.; 3];
    let danger = [20.; 3];
    let times = [0., 2., 4.];
    let mut input = ComfortInput {
        dt: 0.05,
        enabled: true,
        base_tf: 1.45,
        ego_speeds: &speeds,
        lead_speeds: &speeds,
        lead_accel: -1.,
        physical_gaps: &gaps,
        danger_margins: &danger,
        times: &times,
        stop_distance: 6.,
    };
    assert!(state
        .comfort_relief(input)
        .iter()
        .all(|v| (*v - 0.8).abs() < 1e-12));
    input.enabled = false;
    assert_eq!(state.comfort_relief(input), [0.; 3]);
    input.enabled = true;
    assert!(state
        .comfort_relief(input)
        .iter()
        .all(|v| (*v - 0.8).abs() < 1e-12));
    state.update(Observation {
        enabled: true,
        track_id: 2,
        dt: 0.05,
        distance: 60.,
        relative_speed: -1.,
    });
    assert_eq!(state.comfort_relief(input), [0.; 3]);
}

#[test]
fn reserves_are_strict_and_nonfinite_far_horizon_vetoes() {
    let times = [0., 3., 4.];
    assert!(has_reserve(&[20.; 3], &[23.; 3], &[4.; 3], &times, 6.));
    assert!(!has_reserve(&[20.; 3], &[22.; 3], &[4.; 3], &times, 6.));
    assert!(!has_reserve(&[20.; 3], &[23.; 3], &[3.; 3], &times, 6.));
    assert!(!has_reserve(
        &[20.; 3],
        &[23.; 3],
        &[4., 4., f64::NAN],
        &times,
        6.
    ));
    assert!(has_reserve(
        &[20.; 3],
        &[23., 23., 0.],
        &[4., 4., 0.],
        &times,
        6.
    ));
}
