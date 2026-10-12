use super::common::{frame, guidance_text, minutes, number, MediaFrames};
use crate::config::{AMBER, BLUE_SOFT, GREEN, RED};
use crate::models::{NaviDashboardState, NaviLiveState, NaviSpeedInfo};
use crate::renderer::display::{display_speed, format_navi_distance, speed_unit};
use crate::renderer::tokens::{NAVI_MODE_RIGHT_W, NAVI_MODE_RIGHT_X};
use crate::renderer::{Anchor, ClusterUiRenderer, MediaFit, Rect, Result};
use num_traits::AsPrimitive;

impl ClusterUiRenderer {
    pub(super) fn draw_navi_right_band(
        &mut self,
        navi: Option<&NaviLiveState>,
        dashboard: Option<&NaviDashboardState>,
        media: &MediaFrames<'_>,
    ) -> Result<()> {
        let theme = self.current_theme();
        let x = NAVI_MODE_RIGHT_X + 18.0;
        let width = NAVI_MODE_RIGHT_W - 36.0;
        self.draw_text(
            self.text("navigation"),
            x,
            14.0,
            16.0,
            theme.muted,
            Anchor::Left,
        )?;
        if let Some(current) = navi.and_then(|v| v.current.as_ref()) {
            let text = self.ellipsize_text(guidance_text(current), 27.0, width - 120.0)?;
            self.draw_text(&text, x, 42.0, 27.0, theme.text, Anchor::Left)?;
            self.draw_text(
                &format_navi_distance(number(current.distance_m), self.is_metric),
                NAVI_MODE_RIGHT_X + NAVI_MODE_RIGHT_W - 18.0,
                42.0,
                27.0,
                BLUE_SOFT,
                Anchor::Right,
            )?;
        }
        if let Some(next) = navi.and_then(|v| v.next.as_ref()) {
            let line = format!(
                "{} {}  {}",
                self.text("next"),
                format_navi_distance(number(next.distance_m), self.is_metric),
                guidance_text(next)
            );
            let text = self.ellipsize_text(&line, 18.0, width)?;
            self.draw_text(&text, x, 82.0, 18.0, theme.muted, Anchor::Left)?;
        }
        let mut y = 116.0;
        let active: Vec<_> = [
            "image:safety_primary",
            "image:safety_secondary",
            "image:safety_section",
        ]
        .into_iter()
        .filter_map(|key| frame(media, key).filter(|v| v.present))
        .collect();
        if !active.is_empty() {
            let count: f64 = active.len().as_();
            let slot_h = (176.0 / count).min(82.0);
            for frame in active {
                self.draw_navi_media(
                    Some(frame),
                    Rect::new(x, y, width, slot_h - 4.0),
                    MediaFit::default(),
                )?;
                y += slot_h;
            }
        } else if let Some(speed) = navi.and_then(|v| v.speed.as_ref()) {
            y = self.draw_navi_safety_text(speed, x, y, width)?;
        }
        let route_y = (y + 8.0).max(292.0);
        if let Some(route) = navi.and_then(|v| v.route.as_ref()) {
            let progress = (number(route.moved_distance_m) / number(route.total_distance_m.max(1)))
                .clamp(0.0, 1.0);
            self.draw_text(
                self.text("route"),
                x,
                route_y,
                15.0,
                theme.muted,
                Anchor::Left,
            )?;
            let mut value =
                format_navi_distance(number(route.remaining_distance_m), self.is_metric);
            if route.remaining_time_s > 0 {
                super::common::append_format(
                    &mut value,
                    format_args!(" / {:.0} min", minutes(route.remaining_time_s)),
                );
            }
            self.draw_text(
                &value,
                x + width,
                route_y - 2.0,
                20.0,
                theme.text,
                Anchor::Right,
            )?;
            self.gfx.rounded(
                Rect::new(x, route_y + 28.0, width, 8.0),
                1.0,
                8,
                theme.faint,
            )?;
            if progress > 0.0 {
                self.gfx.rounded(
                    Rect::new(x, route_y + 28.0, (width * progress).max(8.0), 8.0),
                    1.0,
                    8,
                    BLUE_SOFT,
                )?;
            }
        }
        self.draw_navi_dashboard_status(dashboard, x, width)
    }

    fn draw_navi_safety_text(
        &mut self,
        speed: &NaviSpeedInfo,
        x: f64,
        mut y: f64,
        width: f64,
    ) -> Result<f64> {
        let theme = self.current_theme();
        for (label, kind, limit, distance) in [
            (
                "SDI",
                speed.sdi_type,
                speed.sdi_speed_limit_kph,
                speed.sdi_distance_m,
            ),
            (
                "SDI 2",
                speed.secondary_sdi_type,
                speed.secondary_sdi_speed_limit_kph,
                speed.secondary_sdi_distance_m,
            ),
        ] {
            if kind.is_none() {
                continue;
            }
            let mut text = label.to_owned();
            if let Some(limit) = limit {
                super::common::append_format(
                    &mut text,
                    format_args!(
                        " {:.0} {}",
                        display_speed(number(limit), self.is_metric),
                        speed_unit(self.is_metric)
                    ),
                );
            }
            if let Some(distance) = distance {
                super::common::append_format(
                    &mut text,
                    format_args!(
                        "  {}",
                        format_navi_distance(number(distance), self.is_metric)
                    ),
                );
            }
            self.draw_text(&text, x, y + 6.0, 24.0, AMBER, Anchor::Left)?;
            y += 40.0;
        }
        if speed.section_active {
            let mut text = self.text("section").to_owned();
            if let Some(average) = speed.section_average_kph {
                super::common::append_format(
                    &mut text,
                    format_args!(
                        " {} {:.0}",
                        self.text("average"),
                        display_speed(average, self.is_metric)
                    ),
                );
            }
            if let Some(distance) = speed.section_remaining_distance_m {
                super::common::append_format(
                    &mut text,
                    format_args!("  {}", format_navi_distance(distance, self.is_metric)),
                );
            }
            let text = self.ellipsize_text(&text, 20.0, width)?;
            self.draw_text(&text, x, y + 4.0, 20.0, theme.text, Anchor::Left)?;
            y += 34.0;
        }
        Ok(y)
    }

    fn draw_navi_dashboard_status(
        &mut self,
        dashboard: Option<&NaviDashboardState>,
        x: f64,
        width: f64,
    ) -> Result<()> {
        let theme = self.current_theme();
        let y = 358.0;
        let Some(dashboard) = dashboard else {
            return self.draw_text(
                &format!("TCP 7714 {}", self.text("waiting")),
                x,
                y,
                18.0,
                AMBER,
                Anchor::Left,
            );
        };
        let error = dashboard.error.as_deref().filter(|v| !v.is_empty());
        let color = if dashboard.connected && dashboard.error.is_none() {
            GREEN
        } else if error.is_some() {
            RED
        } else {
            AMBER
        };
        self.draw_text(
            self.text(if dashboard.connected {
                "connected"
            } else {
                "waiting"
            }),
            x,
            y,
            18.0,
            color,
            Anchor::Left,
        )?;
        let version = if dashboard.app_version.is_empty() {
            "-"
        } else {
            &dashboard.app_version
        };
        self.draw_text(
            &format!(
                "{version}  rev {}  rx {}",
                dashboard.manifest_revision, dashboard.received_count
            ),
            x + width,
            y,
            15.0,
            theme.muted,
            Anchor::Right,
        )?;
        for (text, offset) in [
            (&dashboard.app_status, 27.0),
            (&dashboard.camera_status, 49.0),
            (&dashboard.composition_status, 71.0),
        ] {
            let text = self.ellipsize_text(text, 14.0, width)?;
            self.draw_text(&text, x, y + offset, 14.0, theme.text, Anchor::Left)?;
        }
        let present = dashboard.items.iter().filter(|v| v.present).count();
        let age = dashboard
            .last_received_age_ms
            .map_or_else(|| "-".into(), |age| format!("{age} ms"));
        let footer = format!(
            "STREAMS {present}/28 VALUE  |  AGE {age}  |  {}",
            dashboard.peer
        );
        let footer = self.ellipsize_text(&footer, 13.0, width)?;
        self.draw_text(&footer, x, 458.0, 13.0, theme.muted, Anchor::Left)?;
        if let Some(error) = error {
            let text = self.ellipsize_text(error, 13.0, width)?;
            self.draw_text(&text, x, 436.0, 13.0, RED, Anchor::Left)?;
        }
        Ok(())
    }
}
