use crate::config::{
    cluster_camera_view_is_road_camera, AMBER, GREEN, MAX_SPEED_KPH, RED, TEXT, VEHICLE_NAVI, WHITE,
};
use crate::models::{ClusterUiState, CruiseDisplayState};
use crate::renderer::display::display_speed;
use crate::renderer::tokens::{
    CRUISE_OVERRIDE_APPLY_COLOR, CRUISE_OVERRIDE_LABEL_CENTER_Y, CRUISE_OVERRIDE_LABEL_FONT_SIZE,
    CRUISE_OVERRIDE_SPEED_CENTER_X, CRUISE_OVERRIDE_SPEED_CENTER_Y,
    CRUISE_OVERRIDE_SPEED_FONT_SIZE, CRUISE_SET_SPEED_CENTER_X, CRUISE_SET_SPEED_FONT_SIZE,
    CRUISE_SPEED_CENTER_Y, SPEED_DRIVING_MODE_CENTER_X, SPEED_DRIVING_MODE_CENTER_Y,
    SPEED_DRIVING_MODE_FONT_SIZE, SPEED_EV_CENTER_X, SPEED_EV_CENTER_Y, SPEED_EV_FONT_SIZE,
    SPEED_GAP_CENTER_X, SPEED_GAP_CENTER_Y, SPEED_GAP_FONT_SIZE, SPEED_GEAR_CENTER_X,
    SPEED_GEAR_CENTER_Y, SPEED_GEAR_FONT_SIZE, SPEED_GEAR_H, SPEED_GEAR_W,
    SPEED_LIMIT_SIGN_CENTER_X, SPEED_LIMIT_SIGN_CENTER_Y, SPEED_LIMIT_SIGN_RADIUS,
    SPEED_LIMIT_SIGN_RING_WIDTH, SPEED_MODEL_TRAFFIC_CENTER_X, SPEED_MODEL_TRAFFIC_CENTER_Y,
    SPEED_MODEL_TRAFFIC_ICON_SIZE, SPEED_PANEL_H, SPEED_PANEL_W, SPEED_PANEL_X, SPEED_PANEL_Y,
    SPEED_VALUE_CENTER_X, SPEED_VALUE_CENTER_Y, SPEED_VALUE_FONT_SIZE,
};
use crate::renderer::{
    with_alpha, Anchor, ClusterUiRenderer, IntoColor, Point, Rect, Result, StaticImage, StrokeStyle,
};
use crate::scene::math::clamp;
use num_traits::{AsPrimitive, ToPrimitive};

pub(super) fn cruise_set_visible(state: &ClusterUiState) -> bool {
    state.cruise_kph.is_some() && state.cruise_display_state != CruiseDisplayState::Off
}
fn integer_speed(value: f64, metric: bool) -> String {
    let value = display_speed(value, metric).round_ties_even();
    if value == 0.0 {
        "0".into()
    } else {
        format!("{value:.0}")
    }
}
fn stroke(width: i32, cache: bool) -> StrokeStyle {
    StrokeStyle {
        color: [5, 9, 12, 255],
        width,
        anchor: Anchor::Center,
        cache,
    }
}

impl ClusterUiRenderer {
    /// # Errors
    /// Returns font, texture, TPMS, and native drawing failures.
    pub fn draw_speed_block(&mut self, state: &ClusterUiState, tpms_offset: f64) -> Result<()> {
        self.draw_speed_base(state)?;
        self.draw_driving_mode_indicator(state.driving_mode)?;
        self.draw_model_traffic_state(state.traffic_state)?;
        if let Some(gap) = state.cruise_gap {
            self.draw_text_with_stroke(
                &gap.clamp(1, 4).to_string(),
                SPEED_GAP_CENTER_X,
                SPEED_GAP_CENTER_Y,
                SPEED_GAP_FONT_SIZE,
                WHITE,
                stroke(2, true),
            )?;
        }
        self.draw_speed_gear_badge(state)?;
        if state.ev_mode_valid && state.ev_mode_active {
            self.draw_text_with_stroke(
                "EV",
                SPEED_EV_CENTER_X,
                SPEED_EV_CENTER_Y,
                SPEED_EV_FONT_SIZE,
                GREEN,
                stroke(2, true),
            )?;
        }
        if tpms_offset.abs() > 0.001 {
            self.gfx.push_matrix()?;
            let draw = self
                .gfx
                .translate(tpms_offset, 0.0, 0.0)
                .and_then(|()| self.draw_tpms_status(state));
            let pop = self.gfx.pop_matrix();
            draw?;
            pop?;
        } else {
            self.draw_tpms_status(state)?;
        }
        self.draw_cruise_override(state)?;
        if state.speed_limit_kph.is_some() || state.navi_debug.is_some() {
            self.draw_speed_limit(state.speed_limit_kph.map(AsPrimitive::as_))?;
        }
        Ok(())
    }

    fn draw_speed_base(&mut self, state: &ClusterUiState) -> Result<()> {
        if let Some(texture) = self.static_texture(StaticImage::SpeedBackground) {
            self.gfx.texture_pro(
                texture,
                Rect::new(
                    0.0,
                    0.0,
                    f64::from(texture.width),
                    f64::from(texture.height),
                ),
                Rect::new(SPEED_PANEL_X, SPEED_PANEL_Y, SPEED_PANEL_W, SPEED_PANEL_H),
                Point::default(),
                0.0,
                with_alpha(WHITE, 235),
            )?;
        }
        let speed = integer_speed(
            clamp(
                state.display_speed_kph.unwrap_or(state.speed_kph),
                0.0,
                MAX_SPEED_KPH,
            ),
            self.is_metric,
        );
        let size = if speed.len() <= 2 {
            SPEED_VALUE_FONT_SIZE
        } else {
            SPEED_VALUE_FONT_SIZE * 0.86
        };
        self.draw_text_with_stroke(
            &speed,
            SPEED_VALUE_CENTER_X,
            SPEED_VALUE_CENTER_Y,
            size,
            WHITE,
            StrokeStyle {
                color: [0, 0, 0, 255],
                ..stroke(3, false)
            },
        )?;
        let cruise = if cruise_set_visible(state) {
            integer_speed(state.cruise_kph.unwrap_or(0).as_(), self.is_metric)
        } else {
            "--".into()
        };
        let color = if !cruise_set_visible(state)
            || state.cruise_display_state == CruiseDisplayState::Paused
        {
            self.current_theme().muted
        } else {
            GREEN
        };
        self.draw_text_with_stroke(
            &cruise,
            CRUISE_SET_SPEED_CENTER_X,
            CRUISE_SPEED_CENTER_Y,
            CRUISE_SET_SPEED_FONT_SIZE,
            color,
            StrokeStyle {
                color: [0, 0, 0, 255],
                ..stroke(2, false)
            },
        )
    }

    fn draw_driving_mode_indicator(&mut self, mode: Option<i64>) -> Result<()> {
        let (key, color) = match mode {
            Some(1) => ("driving_mode_eco", [0, 255, 0, 200]),
            Some(2) => ("driving_mode_safe", [255, 165, 0, 200]),
            Some(3) => ("driving_mode_normal", [255, 255, 255, 200]),
            Some(4) => ("driving_mode_sport", [255, 0, 0, 200]),
            _ => return Ok(()),
        };
        let size = if self.language == "ko" {
            SPEED_DRIVING_MODE_FONT_SIZE
        } else {
            22.0
        };
        self.draw_text_with_stroke(
            self.text(key),
            SPEED_DRIVING_MODE_CENTER_X,
            SPEED_DRIVING_MODE_CENTER_Y,
            size,
            color,
            stroke(2, true),
        )
    }
    fn draw_model_traffic_state(&mut self, state: i64) -> Result<()> {
        let kind = match state {
            1 => StaticImage::TrafficRed,
            2 => StaticImage::TrafficGreen,
            _ => return Ok(()),
        };
        let Some(texture) = self.static_texture(kind) else {
            return Ok(());
        };
        let size = SPEED_MODEL_TRAFFIC_ICON_SIZE;
        self.gfx.texture_pro(
            texture,
            Rect::new(
                0.0,
                0.0,
                f64::from(texture.width),
                f64::from(texture.height),
            ),
            Rect::new(
                SPEED_MODEL_TRAFFIC_CENTER_X - size * 0.5,
                SPEED_MODEL_TRAFFIC_CENTER_Y - size * 0.5,
                size,
                size,
            ),
            Point::default(),
            0.0,
            WHITE,
        )
    }
    fn draw_speed_gear_badge(&mut self, state: &ClusterUiState) -> Result<()> {
        let gear: String = state
            .gear_text
            .as_deref()
            .unwrap_or("")
            .trim_matches(openpilot_startup_ui::text::whitespace)
            .to_uppercase()
            .chars()
            .take(2)
            .collect();
        if gear.is_empty() {
            return Ok(());
        }
        let color = if gear == "U" {
            self.current_theme().muted
        } else {
            WHITE
        };
        let outline = if cluster_camera_view_is_road_camera(state.camera_view_mode) {
            color.rgba()
        } else {
            [5, 9, 12, 245]
        };
        let value = crate::renderer::graphics::rect(Rect::new(
            SPEED_GEAR_CENTER_X - SPEED_GEAR_W * 0.5,
            SPEED_GEAR_CENTER_Y - SPEED_GEAR_H * 0.5,
            SPEED_GEAR_W,
            SPEED_GEAR_H,
        ))?;
        let rect = Rect::new(
            f64::from(value.x),
            f64::from(value.y),
            f64::from(value.width),
            f64::from(value.height),
        );
        self.rounded_rect(rect, 8.0, [0, 0, 0, 0], Some(outline), 3.0)?;
        self.draw_text_with_stroke(
            &gear,
            SPEED_GEAR_CENTER_X,
            SPEED_GEAR_CENTER_Y,
            SPEED_GEAR_FONT_SIZE,
            color,
            stroke(2, true),
        )
    }
    fn draw_cruise_override(&mut self, state: &ClusterUiState) -> Result<()> {
        let Some(speed) = state
            .cruise_override_kph
            .filter(|_| cruise_set_visible(state))
        else {
            return Ok(());
        };
        let color = match state.cruise_override_color_mode {
            1 => GREEN,
            4 => AMBER,
            3 => VEHICLE_NAVI,
            2 => CRUISE_OVERRIDE_APPLY_COLOR,
            _ => self.current_theme().text,
        };
        let label = state.cruise_override_label.as_deref().unwrap_or("");
        let count = label
            .chars()
            .count()
            .max(8)
            .to_f64()
            .unwrap_or(f64::INFINITY);
        let size = CRUISE_OVERRIDE_LABEL_FONT_SIZE * (8.0 / count).min(1.0);
        let style = StrokeStyle {
            color: [0, 0, 0, 255],
            ..stroke(2, false)
        };
        self.draw_text_with_stroke(
            label,
            CRUISE_OVERRIDE_SPEED_CENTER_X,
            CRUISE_OVERRIDE_LABEL_CENTER_Y,
            size,
            color,
            style,
        )?;
        self.draw_text_with_stroke(
            &integer_speed(speed, self.is_metric),
            CRUISE_OVERRIDE_SPEED_CENTER_X,
            CRUISE_OVERRIDE_SPEED_CENTER_Y,
            CRUISE_OVERRIDE_SPEED_FONT_SIZE,
            color,
            style,
        )
    }
    fn draw_speed_limit(&mut self, limit: Option<f64>) -> Result<()> {
        let center = Point::new(SPEED_LIMIT_SIGN_CENTER_X, SPEED_LIMIT_SIGN_CENTER_Y);
        self.gfx.circle(center, SPEED_LIMIT_SIGN_RADIUS, RED)?;
        self.gfx.circle(
            center,
            SPEED_LIMIT_SIGN_RADIUS - SPEED_LIMIT_SIGN_RING_WIDTH,
            WHITE,
        )?;
        let text = limit.map_or_else(|| "--".into(), |limit| integer_speed(limit, self.is_metric));
        let size = if text.len() <= 2 { 42.0 } else { 36.0 };
        self.draw_text(&text, center.x, center.y, size, TEXT, Anchor::Center)
    }
}
