use openpilot_cluster::models::{NaviDashboardState, NaviLiveState, NaviTrafficLightInfo};
use openpilot_cluster::renderer::navigation::policy::{
    live_panel_visible, map_frame_present, primary_signal_seconds, signal_active, Signal,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    navi: Option<NaviLiveState>,
    dashboard: Option<NaviDashboardState>,
    traffic: Option<NaviTrafficLightInfo>,
    remaining: [Option<i64>; 5],
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, input] = args.as_slice() else {
        return Err("cluster_navigation_policy_fixture INPUT".into());
    };
    let cases: Vec<Case> = serde_json::from_slice(&std::fs::read(input)?)?;
    let results: Vec<_> = cases
        .into_iter()
        .map(|case| {
            let signals = [
                Signal::Red,
                Signal::Left,
                Signal::Uturn,
                Signal::Straight,
                Signal::Right,
            ];
            let active: Vec<_> = signals
                .into_iter()
                .zip(case.remaining)
                .map(|(signal, seconds)| signal_active(case.traffic.as_ref(), signal, seconds))
                .collect();
            serde_json::json!({
                "visible": live_panel_visible(case.navi.as_ref()),
                "map": map_frame_present(case.dashboard.as_ref()),
                "active": active,
                "primary": primary_signal_seconds(case.traffic.as_ref(), case.remaining),
            })
        })
        .collect();
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
