mod alerts;
mod draw;
mod footer;
mod gauges;
mod plot;
mod plot_draw;
mod replay;
mod route;
mod signal_draw;
mod signals;
mod speed;
mod status;

pub use plot::PlotHistory;
pub use replay::{format_time, ReplayControls};
pub use signal_draw::SignalSide;
pub use signals::SignalState;

use crate::config::{
    cluster_camera_view_is_road_camera, CLUSTER_SCREEN_MODE_DEFAULT,
    CLUSTER_SCREEN_MODE_FULLSCREEN_3D, CLUSTER_SCREEN_MODE_TRIP_REPORT,
};
use crate::models::ClusterUiState;
use crate::renderer::{navigation::policy::live_panel_visible, ClusterUiRenderer};

#[derive(Default)]
pub struct HudState {
    pub signals: SignalState,
    pub plot: PlotHistory,
}

#[must_use]
pub fn effective_screen_mode(requested: i64, state: &ClusterUiState) -> i64 {
    let requested = if requested == CLUSTER_SCREEN_MODE_FULLSCREEN_3D
        && cluster_camera_view_is_road_camera(state.camera_view_mode)
    {
        CLUSTER_SCREEN_MODE_DEFAULT
    } else {
        requested
    };
    if requested != CLUSTER_SCREEN_MODE_DEFAULT {
        return requested;
    }
    let gear = state
        .gear_text
        .as_deref()
        .unwrap_or("")
        .trim_matches(openpilot_startup_ui::text::whitespace)
        .to_uppercase();
    if state.onroad && gear == "P" {
        return CLUSTER_SCREEN_MODE_TRIP_REPORT;
    }
    if state.external_nav_active
        || state
            .navi_dashboard
            .as_ref()
            .is_some_and(|value| value.connected)
        || live_panel_visible(state.navi_live.as_ref())
    {
        CLUSTER_SCREEN_MODE_DEFAULT
    } else {
        CLUSTER_SCREEN_MODE_TRIP_REPORT
    }
}

impl ClusterUiRenderer {
    #[must_use]
    pub fn effective_screen_mode(&self, state: &ClusterUiState) -> i64 {
        effective_screen_mode(self.screen_mode, state)
    }
}
