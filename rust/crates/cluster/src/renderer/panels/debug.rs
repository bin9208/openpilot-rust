use crate::models::ClusterUiState;
use crate::renderer::tokens::{
    NAVI_LIVE_PANEL_H, NAVI_LIVE_PANEL_W, NAVI_LIVE_PANEL_X, NAVI_LIVE_PANEL_Y,
};
use crate::renderer::{Anchor, ClusterUiRenderer, IntoColor, Rect, Result};

type Section = (&'static str, Vec<(&'static str, String)>);

impl ClusterUiRenderer {
    /// # Errors
    /// Returns native drawing and font resource errors.
    pub fn draw_live_debug_panel(&mut self, state: &ClusterUiState) -> Result<()> {
        let theme = self.current_theme();
        let x = self.information_panel_x(NAVI_LIVE_PANEL_X);
        let y = NAVI_LIVE_PANEL_Y;
        let width = (NAVI_LIVE_PANEL_W - 32.0 - 10.0) * 0.5;
        let height = (NAVI_LIVE_PANEL_H - 16.0 - 10.0) * 0.5;
        self.rounded_rect(
            Rect::new(x, y, NAVI_LIVE_PANEL_W, NAVI_LIVE_PANEL_H),
            12.0,
            theme.panel_bg,
            Some(theme.faint.rgba()),
            1.5,
        )?;
        for (index, (title, rows)) in sections(state).into_iter().enumerate() {
            let cx = x + 16.0 + if index % 2 == 0 { 0.0 } else { width + 10.0 };
            let cy = y + 8.0 + if index < 2 { 0.0 } else { height + 10.0 };
            self.rounded_rect(
                Rect::new(cx, cy, width, height),
                12.0,
                theme.route_panel_bg,
                Some(theme.faint.rgba()),
                1.2,
            )?;
            self.draw_text(title, cx + 18.0, cy + 30.0, 18.0, theme.muted, Anchor::Left)?;
            for (index, (label, value)) in rows.iter().enumerate() {
                let text_y = cy + 78.0 + if index == 0 { 0.0 } else { 38.0 };
                self.draw_text(label, cx + 18.0, text_y, 17.0, theme.muted, Anchor::Left)?;
                let value = self.ellipsize_text(value, 17.0, width - 210.0)?;
                self.draw_text(
                    &value,
                    cx + width - 18.0,
                    text_y,
                    17.0,
                    theme.text,
                    Anchor::Right,
                )?;
            }
        }
        Ok(())
    }
}

fn sections(state: &ClusterUiState) -> [Section; 4] {
    let live = state.live_debug.as_ref();
    let valid = live
        .and_then(|v| v.live_torque_valid)
        .map_or("--", |v| if v { "ON" } else { "OFF" });
    [
        (
            "LIVE DELAY",
            vec![(
                "CAL / LAT",
                format!(
                    "{} / {}",
                    optional_percent(live.and_then(|v| v.live_delay_calibration_percent)),
                    optional_seconds(live.and_then(|v| v.live_delay_lateral_s), 2)
                ),
            )],
        ),
        (
            "LIVE TORQUE",
            vec![
                (
                    "STATE",
                    format!(
                        "{valid} / {}",
                        optional_percent(live.and_then(|v| v.live_torque_calibration_percent))
                    ),
                ),
                (
                    "FACT / FRIC",
                    format!(
                        "{} / {}",
                        optional_float(live.and_then(|v| v.live_torque_lat_accel_factor), 2),
                        optional_float(live.and_then(|v| v.live_torque_friction), 2)
                    ),
                ),
            ],
        ),
        (
            "STEERING",
            vec![
                (
                    "SR LIVE / CUSTOM",
                    format!(
                        "{} / {}",
                        optional_float(live.and_then(|v| v.live_steer_ratio), 1),
                        optional_float(live.and_then(|v| v.custom_steer_ratio), 1)
                    ),
                ),
                (
                    "SAD",
                    optional_seconds(live.and_then(|v| v.steer_actuator_delay_s), 2),
                ),
            ],
        ),
        (
            "LATERAL PLAN",
            vec![(
                "DEBUG",
                state
                    .lateral_plan_debug_text
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .unwrap_or("--")
                    .into(),
            )],
        ),
    ]
}
fn optional_percent(value: Option<f64>) -> String {
    value
        .filter(|v| v.is_finite())
        .map_or_else(|| "--%".into(), |value| format!("{value:.0}%"))
}
fn optional_float(value: Option<f64>, digits: usize) -> String {
    value
        .filter(|v| v.is_finite())
        .map_or_else(|| "--".into(), |value| format!("{value:.digits$}"))
}
fn optional_seconds(value: Option<f64>, digits: usize) -> String {
    let text = optional_float(value, digits);
    if text == "--" {
        text
    } else {
        format!("{text} s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_values_do_not_gain_units_and_percent_keeps_source_range() {
        assert_eq!(optional_seconds(Some(f64::NAN), 2), "--");
        assert_eq!(optional_seconds(None, 2), "--");
        assert_eq!(optional_seconds(Some(0.125), 2), "0.12 s");
        assert_eq!(optional_percent(Some(105.0)), "105%");
        assert_eq!(optional_percent(Some(f64::INFINITY)), "--%");
    }
}
