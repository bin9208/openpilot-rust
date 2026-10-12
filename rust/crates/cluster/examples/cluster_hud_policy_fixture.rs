use openpilot_cluster::models::{
    ClusterUiState, DebugPlotSnapshot, NaviDashboardState, NaviGuidanceInfo, NaviLiveState,
};
use openpilot_cluster::renderer::hud::{effective_screen_mode, PlotHistory, SignalState};
use serde::Deserialize;

#[derive(Deserialize)]
struct Input {
    modes: Vec<ModeCase>,
    signals: Vec<Signal>,
    plots: Vec<Plot>,
}
#[derive(Deserialize)]
struct ModeCase {
    mode: i64,
    camera: i64,
    onroad: bool,
    gear: String,
    navigation: i64,
}
#[derive(Deserialize)]
struct Signal {
    now: f64,
    left: bool,
    right: bool,
    highlight: Option<String>,
    offset: Option<f64>,
}
#[derive(Deserialize)]
struct Plot {
    now: f64,
    mode: i64,
    values: [f64; 3],
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("expected INPUT.json")?;
    let input: Input = serde_json::from_slice(&std::fs::read(path)?)?;
    let modes: Vec<_> = input
        .modes
        .into_iter()
        .map(|case| {
            let state = ClusterUiState {
                camera_view_mode: case.camera,
                onroad: case.onroad,
                gear_text: Some(case.gear),
                external_nav_active: case.navigation == 4,
                navi_dashboard: matches!(case.navigation, 1 | 2).then_some(NaviDashboardState {
                    connected: case.navigation == 2,
                    ..Default::default()
                }),
                navi_live: (case.navigation == 3).then_some(NaviLiveState {
                    current: Some(NaviGuidanceInfo::default()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            effective_screen_mode(case.mode, &state)
        })
        .collect();
    let mut signal = SignalState::default();
    let signals:Vec<_>=input.signals.into_iter().map(|case| {
        let state=ClusterUiState{left_signal:case.left,right_signal:case.right,highlight_lane:case.highlight,highlight_lane_offset:case.offset,..Default::default()};
        let lights=signal.lights(&state,case.now);
        serde_json::json!({"lights":lights,"highlight":signal.highlight(&state,lights,case.now)})
    }).collect();
    let mut history = PlotHistory::default();
    let plots:Vec<_>=input.plots.into_iter().map(|case| {
        history.update(Some(&DebugPlotSnapshot{mode:case.mode,title:"oracle".into(),values:case.values}),case.now);
        serde_json::json!({"bounds":history.bounds(),"samples":history.samples().collect::<Vec<_>>()})
    }).collect();
    println!(
        "{}",
        serde_json::json!({"modes":modes,"signals":signals,"plots":plots})
    );
    Ok(())
}
