use crate::config::{AMBER, VEHICLE_NAVI, WHITE};
use crate::models::{ClusterAlert, ClusterUiState};
use crate::renderer::tokens::{
    CAMERA_BACKGROUND_W, CLUSTER_ALERT_SIZE_FULL, CLUSTER_ALERT_SIZE_MID, CLUSTER_ALERT_SIZE_SMALL,
    CLUSTER_ALERT_TEXT_STROKE, NAV_STATUS_CENTER_X, NAV_STATUS_CENTER_Y, NAV_STATUS_FONT_SIZE,
    TOP_STATUS_CENTER_Y,
};
use crate::renderer::{Anchor, ClusterUiRenderer, Result, StrokeStyle};
use openpilot_ui_application::onroad::hud::presentation::{ColorMode, Navigation};

impl ClusterUiRenderer {
    /// # Errors
    /// Returns font fitting, drawing, or native matrix errors.
    pub fn draw_alert_overlay(&mut self, alert: Option<&ClusterAlert>) -> Result<()> {
        let Some(alert) = alert
            .filter(|alert| alert.size > 0 && !(alert.text1.is_empty() && alert.text2.is_empty()))
        else {
            return Ok(());
        };
        self.gfx.push_matrix()?;
        let result = (|| {
            self.gfx
                .scale(self.width / 1920.0, self.height / 480.0, 1.0)?;
            let center = self.driving_panel_offset_design_x() + CAMERA_BACKGROUND_W * 0.5;
            let (width, title_size, detail_size) = if alert.size >= CLUSTER_ALERT_SIZE_FULL {
                (CAMERA_BACKGROUND_W - 100.0, 56.0, 32.0)
            } else if alert.size == CLUSTER_ALERT_SIZE_MID {
                (CAMERA_BACKGROUND_W - 128.0, 48.0, 28.0)
            } else {
                (CAMERA_BACKGROUND_W - 200.0, 42.0, 24.0)
            };
            let (mut title, mut detail) = self.cluster_alert_text(alert);
            if title.is_empty() && !detail.is_empty() {
                std::mem::swap(&mut title, &mut detail);
            }
            let (title, title_size) = self.fit_alert_text(&title, title_size, width, 30.0)?;
            let (detail, detail_size) = self.fit_alert_text(&detail, detail_size, width, 20.0)?;
            let color = match alert.status {
                1 => [255, 174, 82, 255],
                2 => [255, 82, 96, 255],
                _ => [255; 4],
            };
            let style = StrokeStyle {
                color: CLUSTER_ALERT_TEXT_STROKE,
                width: 4,
                anchor: Anchor::Center,
                cache: true,
            };
            if detail.is_empty() {
                self.draw_text_with_stroke(&title, center, 240.0, title_size, color, style)
            } else {
                let small = alert.size == CLUSTER_ALERT_SIZE_SMALL;
                self.draw_text_with_stroke(
                    &title,
                    center,
                    240.0 - if small { 22.0 } else { 30.0 },
                    title_size,
                    color,
                    style,
                )?;
                self.draw_text_with_stroke(
                    &detail,
                    center,
                    240.0 + if small { 27.0 } else { 36.0 },
                    detail_size,
                    WHITE,
                    StrokeStyle { width: 3, ..style },
                )
            }
        })();
        let pop = self.gfx.pop_matrix();
        result?;
        pop
    }

    fn cluster_alert_text(&self, alert: &ClusterAlert) -> (String, String) {
        let keys = match alert.alert_type.as_str() {
            "clusterSelfdriveStartup" => Some(("openpilot_unavailable", "waiting_to_start")),
            "clusterSelfdriveTimeout" => Some(("take_control_immediately", "system_unresponsive")),
            "clusterSelfdriveReboot" => Some(("system_unresponsive", "reboot_device")),
            _ => None,
        };
        keys.map_or_else(
            || {
                (
                    collapse_whitespace(&alert.text1),
                    collapse_whitespace(&alert.text2),
                )
            },
            |(title, detail)| (self.text(title).into(), self.text(detail).into()),
        )
    }
    fn fit_alert_text(
        &mut self,
        text: &str,
        preferred: f64,
        width: f64,
        minimum: f64,
    ) -> Result<(String, f64)> {
        if text.is_empty() {
            return Ok((String::new(), preferred));
        }
        let mut size = preferred;
        while size > minimum && self.measure_text(text, size, None)?.0 > width {
            size -= 2.0;
        }
        Ok((self.ellipsize_text(text, size, width)?, size))
    }

    /// # Errors
    /// Returns native text and texture resource errors.
    pub fn draw_center_clock(&mut self, state: &ClusterUiState, center_x: f64) -> Result<()> {
        let style = StrokeStyle {
            color: [10, 13, 16, 255],
            width: 2,
            anchor: Anchor::Center,
            cache: false,
        };
        if let Some(text) = state
            .center_clock_text
            .as_deref()
            .filter(|text| !text.is_empty())
        {
            self.draw_text_with_stroke(text, center_x, TOP_STATUS_CENTER_Y, 48.0, WHITE, style)?;
        }
        let navigation = Navigation {
            vehicle_available: state.vehicle_navi_available,
            external_active: state.external_nav_active
                || state
                    .navi_dashboard
                    .as_ref()
                    .is_some_and(|dashboard| dashboard.connected),
            owner: &state.navi_owner,
            lifecycle: &state.navi_lifecycle,
        };
        if let Some((label, color)) = navigation.status() {
            self.draw_text_with_stroke(
                label,
                NAV_STATUS_CENTER_X,
                NAV_STATUS_CENTER_Y,
                NAV_STATUS_FONT_SIZE,
                if color == ColorMode::Vehicle {
                    VEHICLE_NAVI
                } else {
                    AMBER
                },
                StrokeStyle {
                    cache: true,
                    ..style
                },
            )?;
        }
        Ok(())
    }
}

fn collapse_whitespace(text: &str) -> String {
    text.split(openpilot_startup_ui::text::whitespace)
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
