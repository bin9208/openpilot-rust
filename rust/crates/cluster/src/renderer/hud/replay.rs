use crate::config::{AMBER, BLUE_SOFT, RED};
use crate::renderer::tokens::{
    CAMERA_OVERLAY_PITCH_OFFSET_MAX_DEG, CAMERA_OVERLAY_PITCH_OFFSET_MIN_DEG,
    CAMERA_OVERLAY_PITCH_TUNE_PANEL_Y, CAMERA_OVERLAY_TUNE_PANEL_Y, CAMERA_OVERLAY_Z_OFFSET_MAX_M,
    CAMERA_OVERLAY_Z_OFFSET_MIN_M, ROUTE_CONTROL_BAR_W, ROUTE_CONTROL_BAR_X, ROUTE_CONTROL_PANEL_H,
    ROUTE_CONTROL_PANEL_W, ROUTE_CONTROL_PANEL_X, ROUTE_CONTROL_PANEL_Y, ROUTE_CONTROL_SEEK_Y,
};
use crate::renderer::{Anchor, ClusterUiRenderer, IntoColor, Point, Rect, Result};
use num_traits::AsPrimitive;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct ReplayControls {
    pub playback_s: f64,
    pub duration_s: f64,
    pub paused: bool,
    pub tuning_visible: bool,
    pub pitch_deg: f64,
    pub height_m: f64,
}

#[must_use]
pub fn format_time(seconds: f64) -> String {
    let total: u64 = seconds.round_ties_even().max(0.0).as_();
    format!("{}:{:02}", total / 60, total % 60)
}

impl ClusterUiRenderer {
    /// # Errors
    /// Returns font and native drawing resource errors.
    pub fn draw_route_replay_controls(&mut self, controls: &ReplayControls) -> Result<()> {
        self.gfx.push_matrix()?;
        let draw = self
            .gfx
            .scale(self.width / 1920.0, self.height / 480.0, 1.0)
            .and_then(|()| self.draw_replay_controls_contents(controls));
        let pop = self.gfx.pop_matrix();
        draw.and(pop)
    }

    fn draw_replay_controls_contents(&mut self, controls: &ReplayControls) -> Result<()> {
        let panel = Rect::new(
            ROUTE_CONTROL_PANEL_X,
            ROUTE_CONTROL_PANEL_Y,
            ROUTE_CONTROL_PANEL_W,
            ROUTE_CONTROL_PANEL_H,
        );
        self.gfx.rounded(panel, 0.20, 12, [2, 5, 10, 188])?;
        self.gfx
            .rounded_lines(panel, 0.20, 12, 1.4, [255, 255, 255, 54])?;
        let duration = controls.duration_s.max(0.001);
        let playback = controls.playback_s.clamp(0.0, duration);
        self.draw_route_slider(
            "seek",
            &format!(
                "{} / {}{}",
                format_time(playback),
                format_time(duration),
                if controls.paused { " PAUSED" } else { "" }
            ),
            playback / duration,
            ROUTE_CONTROL_SEEK_Y,
            BLUE_SOFT,
        )?;
        if controls.tuning_visible {
            self.draw_route_slider(
                "pitch",
                &format!("{:+.1} deg   ; / '", controls.pitch_deg),
                (controls.pitch_deg - CAMERA_OVERLAY_PITCH_OFFSET_MIN_DEG)
                    / (CAMERA_OVERLAY_PITCH_OFFSET_MAX_DEG - CAMERA_OVERLAY_PITCH_OFFSET_MIN_DEG)
                        .max(0.001),
                CAMERA_OVERLAY_PITCH_TUNE_PANEL_Y,
                RED,
            )?;
            self.draw_route_slider(
                "cam z",
                &format!("{:+.2} m   [ / ]", controls.height_m),
                (controls.height_m - CAMERA_OVERLAY_Z_OFFSET_MIN_M)
                    / (CAMERA_OVERLAY_Z_OFFSET_MAX_M - CAMERA_OVERLAY_Z_OFFSET_MIN_M).max(0.001),
                CAMERA_OVERLAY_TUNE_PANEL_Y,
                AMBER,
            )?;
        }
        Ok(())
    }

    fn draw_route_slider(
        &mut self,
        label: &str,
        value: &str,
        ratio: f64,
        center_y: f64,
        fill: impl IntoColor,
    ) -> Result<()> {
        let ratio = ratio.clamp(0.0, 1.0);
        let text = self.current_theme().text;
        self.draw_text(
            label,
            ROUTE_CONTROL_PANEL_X + 26.0,
            center_y - 9.0,
            16.0,
            text,
            Anchor::Left,
        )?;
        self.draw_text(
            value,
            ROUTE_CONTROL_PANEL_X + ROUTE_CONTROL_PANEL_W - 26.0,
            center_y - 9.0,
            16.0,
            text,
            Anchor::Right,
        )?;
        let background = Rect::new(
            ROUTE_CONTROL_BAR_X,
            center_y - 3.0,
            ROUTE_CONTROL_BAR_W,
            6.0,
        );
        let foreground = Rect::new(
            ROUTE_CONTROL_BAR_X,
            center_y - 3.0,
            ROUTE_CONTROL_BAR_W * ratio,
            6.0,
        );
        let knob_x = ROUTE_CONTROL_BAR_X + ROUTE_CONTROL_BAR_W * ratio;
        self.gfx.rounded(background, 1.0, 8, [255, 255, 255, 50])?;
        let [red, green, blue, _] = fill.rgba();
        self.gfx
            .rounded(foreground, 1.0, 8, [red, green, blue, 205])?;
        self.gfx
            .circle(Point::new(knob_x, center_y), 8.5, [red, green, blue, 235])?;
        self.gfx.circle_lines(
            Point::new(knob_x.round_ties_even(), center_y.round_ties_even()),
            9.5,
            [255, 255, 255, 150],
        )
    }
}
