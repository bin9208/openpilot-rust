mod cached;
mod info;
mod navi;
mod plot;
use super::{params::Settings, Input};
use crate::models::{LiveDebugInfo, NaviDebugInfo, NaviGuidanceImage, NaviTrafficLightInfo};

#[derive(Default)]
pub(super) struct Panels {
    pub live: bool,
    pub plot: bool,
    pub navi: bool,
    pub hud: i64,
}
#[derive(Default)]
pub(super) struct DebugState {
    pub panels: Panels,
    next_read: f64,
    cached: Option<LiveDebugInfo>,
    custom_ratio: Option<f64>,
    actuator_delay: Option<f64>,
    pub plot_mode: i64,
    pub navi_image: Option<NaviGuidanceImage>,
    pub navi_debug: Option<NaviDebugInfo>,
    held_light: Option<(NaviTrafficLightInfo, f64)>,
}
impl DebugState {
    pub fn refresh(&mut self, params: Option<&Settings>, now: f64) {
        if now < self.next_read {
            return;
        }
        self.next_read = now + 1.0;
        let Some(params) = params else {
            return;
        };
        self.custom_ratio = params.float("CustomSR", 0.1);
        self.actuator_delay = params.float("SteerActuatorDelay", 0.01);
        self.plot_mode = params.integer("ShowPlotMode", 0);
        self.cached = cached::read(params);
        self.refresh_navigation(params, now);
    }
}
impl Input {
    pub fn set_debug_panels_enabled(&mut self, live: bool, plot: bool, navi: bool) {
        let previous = &self.debug.panels;
        if previous.live != live || previous.plot != plot || previous.navi != navi {
            self.debug.next_read = 0.0;
        }
        self.debug.panels.live = live;
        self.debug.panels.plot = plot;
        self.debug.panels.navi = navi;
    }
    pub fn set_hud_debug_mode(&mut self, mode: i64) {
        let mode = mode.clamp(0, 3);
        if self.debug.panels.hud != mode {
            self.debug.panels.hud = mode;
            self.debug.next_read = 0.0;
        }
    }
}
