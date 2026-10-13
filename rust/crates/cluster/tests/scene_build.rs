use openpilot_cluster::{
    config::light_theme,
    models::{ClusterUiState, DetectedVehicle, RadarPoint},
    scene::SceneBuilder,
};

#[test]
fn scene_state_key_tracks_geometry_and_ignores_unrelated_hud_state() {
    let mut state = ClusterUiState::default();
    let key = openpilot_cluster::scene::SceneStateKey::from_state(&state);
    assert!(key.matches(&state));
    state.network_connected = true;
    assert!(key.matches(&state));
    state.lanes[0].width += 1;
    assert!(!key.matches(&state));
    state.lanes[0].width -= 1;
    state.left_blindspot = true;
    assert!(!key.matches(&state));
}

#[test]
fn default_scene_keeps_empty_road_surface_and_full_lane_geometry() {
    let scene =
        SceneBuilder::default().build(&ClusterUiState::default(), true, &light_theme(), None);
    assert!(scene.road_surface.left.is_empty());
    assert!(!scene.lane_markings.is_empty());
    assert!(!scene.planned_path.is_empty());
    assert_eq!(scene.vehicles.len(), 1);
    assert_eq!(scene.camera.fovy_deg.to_bits(), 31.0_f64.to_bits());
}

#[test]
fn radar_merge_uses_label_order_for_equal_signed_zero_coordinates() {
    let state = ClusterUiState {
        radar_points: vec![
            RadarPoint {
                label: "A".into(),
                source: "liveTracks".into(),
                longitudinal_m: 0.0,
                lateral_m: 3.0,
                ..RadarPoint::default()
            },
            RadarPoint {
                label: "B".into(),
                source: "liveTracks".into(),
                longitudinal_m: -0.0,
                lateral_m: 3.0,
                ..RadarPoint::default()
            },
        ],
        ..ClusterUiState::default()
    };
    let scene = SceneBuilder::default().build(&state, true, &light_theme(), None);
    assert_eq!(scene.radar_points[0].label, "A+1");
}

#[test]
fn raw_corner_input_filters_summary_but_keeps_cut_in_track_once() {
    let state = ClusterUiState {
        radar_points: vec![RadarPoint {
            label: "corner".into(),
            source: "cornerRadar".into(),
            longitudinal_m: 22.0,
            lateral_m: 4.0,
            ..RadarPoint::default()
        }],
        detected_vehicles: vec![
            DetectedVehicle {
                label: "LF".into(),
                source: "carState".into(),
                longitudinal_m: 22.0,
                lateral_m: 4.0,
                ..DetectedVehicle::default()
            },
            DetectedVehicle {
                label: "CUT".into(),
                cut_in: true,
                radar_track_id: Some(42),
                longitudinal_m: 22.0,
                lateral_m: 3.0,
                ..DetectedVehicle::default()
            },
            DetectedVehicle {
                label: "L1".into(),
                source: "radarState".into(),
                radar_track_id: Some(42),
                longitudinal_m: 22.0,
                lateral_m: 3.0,
                ..DetectedVehicle::default()
            },
        ],
        ..ClusterUiState::default()
    };
    let scene = SceneBuilder::default().build(&state, true, &light_theme(), None);
    assert!(scene.vehicles.iter().any(|vehicle| vehicle.label == "CUT"));
    assert!(!scene
        .vehicles
        .iter()
        .any(|vehicle| matches!(vehicle.label.as_str(), "LF" | "L1")));
}
