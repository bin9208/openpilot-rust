use openpilot_cluster::{
    models::{ClusterUiState, NaviLiveState, NaviSpeedInfo},
    navi_simulator::merge_overlay_state,
};

#[test]
fn overlay_keeps_vehicle_limit_and_motion_but_fills_empty_clock() {
    let base = ClusterUiState {
        speed_kph: 45.0,
        speed_limit_kph: Some(80),
        speed_limit_source: Some("v".into()),
        center_clock_text: Some(String::new()),
        ..ClusterUiState::default()
    };
    let overlay = ClusterUiState {
        speed_kph: 1.0,
        center_clock_text: Some("12:34:56".into()),
        navi_live: Some(NaviLiveState {
            speed: Some(NaviSpeedInfo {
                road_limit_kph: Some(30),
                ..NaviSpeedInfo::default()
            }),
            ..NaviLiveState::default()
        }),
        ..ClusterUiState::default()
    };
    let result = merge_overlay_state(&base, &overlay);
    assert_eq!(result.speed_kph.to_bits(), 45.0_f64.to_bits());
    assert_eq!(result.speed_limit_kph, Some(80));
    assert_eq!(result.speed_limit_source.as_deref(), Some("v"));
    assert_eq!(result.center_clock_text.as_deref(), Some("12:34:56"));
    assert_eq!(result.navi_live, overlay.navi_live);
}
