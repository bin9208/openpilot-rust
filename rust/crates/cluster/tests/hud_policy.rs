use openpilot_cluster::models::{ClusterUiState, DebugPlotSnapshot, NaviDashboardState};
use openpilot_cluster::renderer::hud::{effective_screen_mode, PlotHistory, SignalState};

#[test]
fn parking_report_precedes_connected_navigation_and_camera_rejects_fullscreen() {
    let mut state = ClusterUiState {
        onroad: true,
        gear_text: Some(" p ".into()),
        navi_dashboard: Some(NaviDashboardState {
            connected: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(effective_screen_mode(0, &state), 5);
    state.gear_text = Some("D".into());
    assert_eq!(effective_screen_mode(0, &state), 0);
    state.camera_view_mode = 2;
    assert_eq!(effective_screen_mode(-1, &state), 0);
    assert_eq!(effective_screen_mode(2, &state), 2);
}

#[test]
fn hazard_phase_resets_single_turn_and_highlight_inherits_signal_phase() {
    let mut signals = SignalState::default();
    let mut state = ClusterUiState {
        left_signal: true,
        highlight_lane: Some("left".into()),
        highlight_lane_offset: Some(-1.0),
        ..Default::default()
    };
    let lit = signals.lights(&state, 10.0);
    assert_eq!(lit, (true, false));
    assert!(signals.highlight(&state, lit, 10.0));
    assert_eq!(signals.lights(&state, 10.5), (false, false));
    state.right_signal = true;
    assert_eq!(signals.lights(&state, 10.6), (true, true));
    state.right_signal = false;
    assert_eq!(signals.lights(&state, 10.7), (true, false));
    let lit = signals.lights(&state, 11.3);
    assert!(!signals.highlight(&state, lit, 11.3));
    state.left_signal = false;
    let lit = signals.lights(&state, 11.4);
    assert!(!signals.highlight(&state, lit, 11.4));
    assert!(signals.highlight(&state, lit, 11.7));
}

#[test]
fn plot_sample_period_mode_reset_and_ring_eviction_preserve_bounds() {
    let mut history = PlotHistory::default();
    let mut plot = DebugPlotSnapshot {
        mode: 1,
        title: "test".into(),
        values: [3.0, 4.0, -6.0],
    };
    history.update(Some(&plot), 0.0);
    history.update(Some(&plot), 0.049);
    assert_eq!(history.len(), 1);
    history.update(Some(&plot), 0.05);
    assert_eq!(history.len(), 2);
    let (low, high) = history.bounds();
    assert_eq!(
        (low.to_bits(), high.to_bits()),
        ((-6.0_f64).to_bits(), 4.0_f64.to_bits())
    );
    for index in 0..360 {
        plot.values = [0.0, f64::NAN, f64::INFINITY];
        history.update(Some(&plot), 1.0 + f64::from(index));
    }
    assert_eq!(history.len(), 360);
    let (low, high) = history.bounds();
    assert_eq!(
        (low.to_bits(), high.to_bits()),
        ((-2.0_f64).to_bits(), 2.0_f64.to_bits())
    );
    assert_eq!(
        history.samples().next().unwrap()[1].to_bits(),
        0.0_f64.to_bits()
    );
    plot.mode = 2;
    history.update(Some(&plot), 400.0);
    assert_eq!(history.len(), 1);
    history.update(None, 401.0);
    assert!(history.is_empty());
}
