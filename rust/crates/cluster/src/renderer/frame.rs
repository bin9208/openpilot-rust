use super::{ClusterUiRenderer, FrameClock, Result};
use crate::{config::cluster_camera_view_is_road_camera, models::ClusterUiState};

impl ClusterUiRenderer {
    /// # Errors
    /// Returns native drawing errors from the active frame target.
    pub fn render_at(
        &mut self,
        state: &ClusterUiState,
        clock: FrameClock,
        lights: Option<(bool, bool)>,
    ) -> Result<()> {
        let lights = lights.unwrap_or_else(|| self.hud.signals.lights(state, clock.perf_counter));
        let start = self.profile_start();
        if matches!(self.screen_mode, 3 | 6) {
            let clear_start = self.profile_start();
            self.gfx.clear(self.current_theme().bg)?;
            self.profile_add("render_world.clear_background", clear_start);
        } else {
            self.render_world(state, clock, lights)?;
        }
        self.profile_add("render.world", start);
        let start = self.profile_start();
        self.draw_hud(state, clock, Some(lights))?;
        self.profile_add("render.hud", start);
        let start = self.profile_start();
        self.draw_alert_overlay(state.alert.as_ref())?;
        self.profile_add("render.alert", start);
        Ok(())
    }
    /// # Errors
    /// Returns frame rendering or native window initialization errors.
    pub fn render_frame(&mut self, state: &ClusterUiState) -> Result<()> {
        self.render_frame_at(state, FrameClock::now()?, None)
    }
    /// # Errors
    /// Returns drawing errors; always ends the normal window frame.
    pub fn render_frame_at(
        &mut self,
        state: &ClusterUiState,
        clock: FrameClock,
        lights: Option<(bool, bool)>,
    ) -> Result<()> {
        self.open(false)?;
        self.prepare_trip_report_cache(state, clock.monotonic)?;
        let start = self.profile_start();
        self.gfx.backend()?.raw_begin()?;
        self.profile_add("render_frame.begin_drawing", start);
        let start = self.profile_start();
        let result = self.render_at(state, clock, lights);
        self.profile_add("render_frame.render", start);
        let start = self.profile_start();
        self.gfx.backend()?.raw_end();
        self.profile_add("render_frame.end_drawing", start);
        result?;
        self.flush_pending_strokes()
    }
    #[must_use]
    pub fn should_close(&self) -> bool {
        self.gfx
            .native
            .as_ref()
            .is_some_and(openpilot_startup_ui::renderer::Renderer::should_close)
    }
    pub fn set_target_fps(&mut self, fps: i32) {
        self.options.target_fps = fps.max(0);
        let start = self.profile_start();
        if let Some(native) = self.gfx.native.as_mut() {
            native.set_target_fps(self.options.target_fps);
            self.profile_add("renderer.set_target_fps", start);
        }
    }
    fn render_world(
        &mut self,
        state: &ClusterUiState,
        clock: FrameClock,
        lights: (bool, bool),
    ) -> Result<()> {
        let road = cluster_camera_view_is_road_camera(state.camera_view_mode);
        if !road || !state.onroad {
            self.close_live_road_camera()?;
        }
        let theme = self.current_theme();
        let start = self.profile_start();
        let highlight = self
            .hud
            .signals
            .highlight(state, lights, clock.perf_counter);
        let samples = &mut self.profile_samples;
        let mut add = |name: &str, elapsed: f64| samples.push((name.into(), elapsed));
        let callback: &mut dyn FnMut(&str, f64) = &mut add;
        let profile = self.profile_enabled.then_some(callback);
        let scene = self.scene_cache.get(state, highlight, &theme, profile);
        self.profile_add("render_world.build_scene", start);
        let start = self.profile_start();
        self.gfx.clear(theme.bg)?;
        self.profile_add("render_world.clear_background", start);
        if road {
            self.camera_overlay.wide = self.select_camera_overlay_stream(state, clock.monotonic)?;
            let projection = self.camera_overlay_projection(state);
            let start = self.profile_start();
            self.draw_camera_background(state, projection.as_ref(), clock.monotonic)?;
            self.profile_add("render_world.camera_background", start);
            let start = self.profile_start();
            self.draw_camera_projected_overlay(&scene, state, projection.as_ref())?;
            self.profile_add("render_world.camera_projected_overlay", start);
        } else {
            self.camera_overlay.wide = false;
            let start = self.profile_start();
            self.draw_scene(&scene, state)?;
            self.profile_add("render_world.draw_scene", start);
        }
        Ok(())
    }
}
