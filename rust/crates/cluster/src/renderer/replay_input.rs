use super::{
    hud::ReplayControls,
    tokens::{
        CAMERA_OVERLAY_PITCH_OFFSET_DEFAULT_DEG, CAMERA_OVERLAY_PITCH_OFFSET_MAX_DEG,
        CAMERA_OVERLAY_PITCH_OFFSET_MIN_DEG, CAMERA_OVERLAY_PITCH_OFFSET_STEP_DEG,
        CAMERA_OVERLAY_PITCH_TUNE_PANEL_Y, CAMERA_OVERLAY_TUNE_PANEL_Y,
        CAMERA_OVERLAY_Z_OFFSET_DEFAULT_M, CAMERA_OVERLAY_Z_OFFSET_MAX_M,
        CAMERA_OVERLAY_Z_OFFSET_MIN_M, CAMERA_OVERLAY_Z_OFFSET_STEP_M, ROUTE_CONTROL_BAR_W,
        ROUTE_CONTROL_BAR_X, ROUTE_CONTROL_SEEK_Y,
    },
    ClusterUiRenderer, FrameClock, Point, Rect, Result,
};
use crate::{
    config::cluster_camera_view_is_road_camera, models::ClusterUiState, scene::math::clamp,
};

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct ReplayInput {
    pub seek_s: Option<f64>,
    pub corner_lateral_offset_m: f64,
    pub control_active: bool,
}
impl ClusterUiRenderer {
    #[must_use]
    pub fn route_replay_control_input(
        &mut self,
        _playback_s: f64,
        duration_s: f64,
        corner_lateral_offset_m: f64,
    ) -> ReplayInput {
        let idle = ReplayInput {
            seek_s: None,
            corner_lateral_offset_m,
            control_active: false,
        };
        if duration_s <= 0.0 {
            return idle;
        }
        let Some(native) = self.gfx.native.as_ref() else {
            return idle;
        };
        let mouse = native.mouse_position();
        let point = Point::new(
            f64::from(mouse.x) / (self.width / 1920.0).max(0.001),
            f64::from(mouse.y) / (self.height / 480.0).max(0.001),
        );
        let down = native.mouse_button_down(0);
        let pressed = [91, 93, 59, 39, 48].map(|key| native.key_started(key));
        let active = self.camera_overlay.tuning_visible && self.tuning_input(point, down, pressed);
        let seek = down && bar(ROUTE_CONTROL_SEEK_Y).contains(point);
        ReplayInput {
            seek_s: seek.then(|| ratio(point.x) * duration_s),
            corner_lateral_offset_m,
            control_active: active || seek,
        }
    }
    #[must_use]
    pub fn route_replay_mouse_down(&self) -> bool {
        self.gfx
            .native
            .as_ref()
            .is_some_and(|native| native.mouse_button_down(0))
    }
    /// # Errors
    /// Returns drawing failures while always ending the replay window frame.
    pub fn render_route_replay_frame_at(
        &mut self,
        state: &ClusterUiState,
        clock: FrameClock,
        playback_s: f64,
        duration_s: f64,
        paused: bool,
    ) -> Result<()> {
        self.open(false)?;
        self.prepare_trip_report_cache(state, clock.monotonic)?;
        let start = self.profile_start();
        self.gfx.backend()?.raw_begin()?;
        self.profile_add("render_route_frame.begin_drawing", start);
        let result = (|| -> Result<()> {
            let start = self.profile_start();
            self.render_at(state, clock, None)?;
            self.profile_add("render_route_frame.render", start);
            let controls = ReplayControls {
                playback_s,
                duration_s,
                paused,
                tuning_visible: self.camera_overlay.tuning_visible
                    && cluster_camera_view_is_road_camera(state.camera_view_mode),
                pitch_deg: self.camera_overlay.pitch_offset_deg,
                height_m: self.camera_overlay.z_offset_m,
            };
            let start = self.profile_start();
            self.draw_route_replay_controls(&controls)?;
            self.profile_add("render_route_frame.controls", start);
            Ok(())
        })();
        let start = self.profile_start();
        self.gfx.backend()?.raw_end();
        self.profile_add("render_route_frame.end_drawing", start);
        let cleanup = self.flush_pending_strokes();
        result.and(cleanup)
    }
    /// # Errors
    /// Returns drawing or clock conversion failures.
    pub fn render_route_replay_frame(
        &mut self,
        state: &ClusterUiState,
        playback_s: f64,
        duration_s: f64,
        paused: bool,
    ) -> Result<()> {
        self.render_route_replay_frame_at(state, FrameClock::now()?, playback_s, duration_s, paused)
    }
    /// # Errors
    /// Returns an unopened window or screenshot export failure.
    pub fn write_window_png(&mut self, path: &std::path::Path) -> Result<()> {
        Ok(self.gfx.backend()?.screen_screenshot(path)?)
    }
    fn tuning_input(&mut self, point: Point, down: bool, pressed: [bool; 5]) -> bool {
        let [left, right, pitch_down, pitch_up, reset] = pressed;
        let changed = pressed.into_iter().any(|value| value);
        let overlay = &mut self.camera_overlay;
        if left {
            overlay.z_offset_m -= CAMERA_OVERLAY_Z_OFFSET_STEP_M;
        }
        if right {
            overlay.z_offset_m += CAMERA_OVERLAY_Z_OFFSET_STEP_M;
        }
        if pitch_down {
            overlay.pitch_offset_deg -= CAMERA_OVERLAY_PITCH_OFFSET_STEP_DEG;
        }
        if pitch_up {
            overlay.pitch_offset_deg += CAMERA_OVERLAY_PITCH_OFFSET_STEP_DEG;
        }
        if reset {
            overlay.z_offset_m = CAMERA_OVERLAY_Z_OFFSET_DEFAULT_M;
            overlay.pitch_offset_deg = CAMERA_OVERLAY_PITCH_OFFSET_DEFAULT_DEG;
        }
        let tune_z = down && bar(CAMERA_OVERLAY_TUNE_PANEL_Y).contains(point);
        let tune_pitch = down && bar(CAMERA_OVERLAY_PITCH_TUNE_PANEL_Y).contains(point);
        if tune_z {
            overlay.z_offset_m = CAMERA_OVERLAY_Z_OFFSET_MIN_M
                + ratio(point.x) * (CAMERA_OVERLAY_Z_OFFSET_MAX_M - CAMERA_OVERLAY_Z_OFFSET_MIN_M);
        }
        if tune_pitch {
            overlay.pitch_offset_deg = CAMERA_OVERLAY_PITCH_OFFSET_MIN_DEG
                + ratio(point.x)
                    * (CAMERA_OVERLAY_PITCH_OFFSET_MAX_DEG - CAMERA_OVERLAY_PITCH_OFFSET_MIN_DEG);
        }
        if changed || tune_z || tune_pitch {
            overlay.z_offset_m = clamp(
                overlay.z_offset_m,
                CAMERA_OVERLAY_Z_OFFSET_MIN_M,
                CAMERA_OVERLAY_Z_OFFSET_MAX_M,
            );
            overlay.pitch_offset_deg = clamp(
                overlay.pitch_offset_deg,
                CAMERA_OVERLAY_PITCH_OFFSET_MIN_DEG,
                CAMERA_OVERLAY_PITCH_OFFSET_MAX_DEG,
            );
        }
        changed || tune_z || tune_pitch
    }
}
fn bar(y: f64) -> Rect {
    Rect::new(ROUTE_CONTROL_BAR_X, y - 8.0, ROUTE_CONTROL_BAR_W, 16.0)
}
fn ratio(x: f64) -> f64 {
    clamp(
        (x - ROUTE_CONTROL_BAR_X) / ROUTE_CONTROL_BAR_W.max(1.0),
        0.0,
        1.0,
    )
}
