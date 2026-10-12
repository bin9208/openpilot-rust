use openpilot_cluster::{
    models::ClusterUiState,
    renderer::{
        camera_overlay::{camera_for_state, projection, screen_xy, visible_runs},
        Rect,
    },
    scene::Vec3,
};

#[test]
fn camera_lookup_preserves_ordered_source_fallbacks() {
    let mut state = ClusterUiState {
        camera_device_type: Some(" MICI ".into()),
        camera_sensor: Some(" OS04C10 ".into()),
        ..ClusterUiState::default()
    };
    assert_eq!(
        camera_for_state(&state, false).width.to_bits(),
        1344.0_f64.to_bits()
    );
    assert_eq!(
        camera_for_state(&state, true).focal_length.to_bits(),
        425.25_f64.to_bits()
    );
    state.camera_device_type = Some("neo".into());
    state.camera_sensor = None;
    assert_eq!(
        camera_for_state(&state, true).focal_length.to_bits(),
        910.0_f64.to_bits()
    );
    state.camera_device_type = Some("missing".into());
    state.camera_sensor = Some("os04c10".into());
    assert_eq!(
        camera_for_state(&state, false).focal_length.to_bits(),
        1141.5_f64.to_bits()
    );
    state.camera_sensor = Some("missing".into());
    assert_eq!(
        camera_for_state(&state, false).focal_length.to_bits(),
        2648.0_f64.to_bits()
    );
}

#[test]
fn physical_projection_rejects_ego_depth_and_outside_clip() {
    let state = ClusterUiState::default();
    let view = projection(&state, Rect::new(0., 0., 1124., 480.), false, 0.);
    let center = screen_xy(Vec3::new(0., 34.18, 0.), &view, 0., 0.).expect("forward point visible");
    assert!((center.x - 562.).abs() < 1e-10);
    assert!(screen_xy(Vec3::new(0., 4.68, 0.), &view, 0., 0.).is_none());
    assert!(screen_xy(Vec3::new(100., 34.18, 0.), &view, 0., 0.).is_none());
    assert!(screen_xy(Vec3::new(f64::NAN, 34.18, 0.), &view, 0., 0.).is_none());
}

#[test]
fn camera_height_uses_source_finite_clamp() {
    for (height, expected) in [(-2.0_f64, 0.5_f64), (8., 3.), (f64::NAN, 1.22)] {
        let state = ClusterUiState {
            road_transform_trans: Some([0., 0., height]),
            ..ClusterUiState::default()
        };
        let view = projection(&state, Rect::new(0., 0., 1124., 480.), false, 0.);
        assert_eq!(view.camera_height_m.to_bits(), expected.to_bits());
    }
}

#[test]
fn strip_batches_never_bridge_rejected_endpoint_pairs() {
    let ranges: Vec<_> = visible_runs(&[1, 1, 0, 1, 1, 1, 0, 1]).collect();
    assert_eq!(ranges, [0..2, 3..6]);
    assert_eq!(visible_runs(&[0, 0, 1, 0]).count(), 0);
}
