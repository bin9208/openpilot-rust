use super::{CardStyle, Metric};
use crate::config::{AMBER, BLUE_SOFT, RED};
use crate::models::{ClusterUiState, TripReportState};
use crate::renderer::display::{display_speed, format_trip_distance, speed_unit};
use crate::renderer::tokens::{
    NAVI_LIVE_PANEL_H, NAVI_LIVE_PANEL_W, NAVI_LIVE_PANEL_X, NAVI_LIVE_PANEL_Y,
    TRIP_REPORT_PANEL_H, TRIP_REPORT_PANEL_W, TRIP_REPORT_PANEL_X, TRIP_REPORT_PANEL_Y,
};
use crate::renderer::{
    Anchor, ClusterUiRenderer, Error, IntoColor, Point, Rect, Result, SystemPanelOptions,
};
use num_traits::ToPrimitive;

impl ClusterUiRenderer {
    /// # Errors
    /// Returns native drawing and font resource errors.
    pub fn draw_system_dashboard_panel(&mut self, state: &ClusterUiState) -> Result<()> {
        let theme = self.current_theme();
        let system_info = self.stats.sample(self.started.elapsed().as_secs_f64());
        let x = self.information_panel_x(NAVI_LIVE_PANEL_X);
        let y = NAVI_LIVE_PANEL_Y;
        let detail_x = x + 16.0;
        let health_x = detail_x + 474.0 + 10.0;
        self.rounded_rect(
            Rect::new(x, y, NAVI_LIVE_PANEL_W, NAVI_LIVE_PANEL_H),
            12.0,
            theme.panel_bg,
            Some(theme.faint.rgba()),
            1.5,
        )?;
        self.draw_system_stats_panel(
            state,
            SystemPanelOptions {
                panel_x: Some(detail_x),
                panel_y: y + 8.0,
                panel_w: 474.0,
                panel_h: Some(NAVI_LIVE_PANEL_H - 16.0),
                show_runtime_info: true,
                title_text: "DETAIL",
                stats: Some(&system_info),
                ..SystemPanelOptions::default()
            },
        )?;
        self.draw_system_health_card(
            state,
            &system_info,
            Rect::new(
                health_x,
                y + 8.0,
                x + NAVI_LIVE_PANEL_W - 16.0 - health_x,
                NAVI_LIVE_PANEL_H - 16.0,
            ),
            CardStyle::from_theme(&theme),
        )
    }

    /// # Errors
    /// Returns invalid duration or native drawing and font resource errors.
    pub fn draw_trip_report_panel_contents(&mut self, state: &ClusterUiState) -> Result<()> {
        let default = TripReportState::default();
        let report = state.trip_report.as_ref().unwrap_or(&default);
        let system_info = self.stats.sample(self.started.elapsed().as_secs_f64());
        let theme = self.current_theme();
        let style = CardStyle::from_theme(&theme);
        let x = self.information_panel_x(TRIP_REPORT_PANEL_X);
        let y = TRIP_REPORT_PANEL_Y;
        self.rounded_rect(
            Rect::new(x, y, TRIP_REPORT_PANEL_W, TRIP_REPORT_PANEL_H),
            12.0,
            theme.panel_bg,
            Some(style.outline),
            1.5,
        )?;
        let sx = x + 16.0;
        let sy = y + 8.0;
        let sw = 474.0;
        let sh = TRIP_REPORT_PANEL_H - 16.0;
        self.rounded_rect(
            Rect::new(sx, sy, sw, sh),
            10.0,
            style.background,
            Some(style.outline),
            1.2,
        )?;
        self.draw_text(
            self.text("trip_summary"),
            sx + 18.0,
            sy + 31.0,
            22.0,
            style.muted,
            Anchor::Left,
        )?;
        self.draw_text(
            self.text("time"),
            sx + 18.0,
            sy + 82.0,
            19.0,
            style.muted,
            Anchor::Left,
        )?;
        self.draw_text(
            &trip_format_time(report.duration_s)?,
            sx + sw - 18.0,
            sy + 82.0,
            35.0,
            [255, 177, 105],
            Anchor::Right,
        )?;
        self.gfx.line(
            Point::new(sx + 18.0, sy + 114.0),
            Point::new(sx + sw - 18.0, sy + 114.0),
            1.0,
            style.outline,
        )?;
        let summary = Rect::new(sx, sy, sw, sh);
        self.draw_trip_values(report, summary, style)?;
        self.draw_trip_events(report, summary, style)?;
        let system_x = sx + sw + 10.0;
        self.draw_system_health_card(
            state,
            &system_info,
            Rect::new(system_x, sy, x + TRIP_REPORT_PANEL_W - 16.0 - system_x, sh),
            style,
        )
    }

    fn draw_trip_values(
        &mut self,
        report: &TripReportState,
        bounds: Rect,
        style: CardStyle,
    ) -> Result<()> {
        let Rect {
            x: sx,
            y: sy,
            width: sw,
            ..
        } = bounds;
        let metric_width = (sw - 54.0) * 0.5;
        let unit = speed_unit(self.is_metric);
        let values = [
            (
                "distance",
                format_trip_distance(report.distance_m, self.is_metric),
                "",
            ),
            (
                "average_speed",
                format!(
                    "{:.1}",
                    display_speed(report.average_speed_kph, self.is_metric)
                ),
                unit,
            ),
            (
                "max_speed",
                format!("{:.1}", display_speed(report.max_speed_kph, self.is_metric)),
                unit,
            ),
            (
                "auto_drive",
                format!("{:.0}", report.auto_ratio_percent),
                "%",
            ),
            (
                "max_accel",
                format!("{:+.2}", report.max_accel_mps2),
                "m/s²",
            ),
            (
                "max_decel",
                format!("{:+.2}", report.max_decel_mps2),
                "m/s²",
            ),
        ];
        for (index, (key, value, unit)) in values.iter().enumerate() {
            let left = sx
                + 18.0
                + if index % 2 == 0 {
                    0.0
                } else {
                    metric_width + 18.0
                };
            let top = sy + [138.0, 218.0, 298.0][index / 2];
            self.draw_trip_metric(
                Metric {
                    bounds: Rect::new(left, top, metric_width, 0.0),
                    label: self.text(key),
                    value,
                    unit,
                },
                style,
            )?;
        }
        Ok(())
    }

    fn draw_trip_events(
        &mut self,
        report: &TripReportState,
        bounds: Rect,
        style: CardStyle,
    ) -> Result<()> {
        let Rect {
            x: sx,
            y: sy,
            width: sw,
            ..
        } = bounds;
        let event_width = (sw - 56.0) / 3.0;
        let label_size = if self.language == "ko" { 16.0 } else { 13.0 };
        for (index, (key, count, color)) in [
            ("hard_accel", report.hard_accel_count, AMBER),
            ("hard_brake", report.hard_brake_count, RED),
            ("hard_corner", report.hard_corner_count, BLUE_SOFT),
        ]
        .into_iter()
        .enumerate()
        {
            let left = sx + 18.0 + [0.0, 1.0, 2.0][index] * (event_width + 10.0);
            self.rounded_rect(
                Rect::new(left, sy + 390.0, event_width, 47.0),
                7.0,
                style.target,
                Some(color.rgba()),
                1.2,
            )?;
            self.draw_text(
                self.text(key),
                left + 9.0,
                sy + 390.0 + 23.5,
                label_size,
                style.muted,
                Anchor::Left,
            )?;
            self.draw_text(
                &count.to_string(),
                left + event_width - 9.0,
                sy + 390.0 + 23.5,
                23.0,
                color,
                Anchor::Right,
            )?;
        }
        Ok(())
    }

    fn draw_trip_metric(&mut self, metric: Metric<'_>, style: CardStyle) -> Result<()> {
        self.draw_text(
            metric.label,
            metric.bounds.x,
            metric.bounds.y,
            18.0,
            style.muted,
            Anchor::Left,
        )?;
        self.draw_text(
            metric.value,
            metric.bounds.x,
            metric.bounds.y + 32.0,
            29.0,
            style.text,
            Anchor::Left,
        )?;
        if !metric.unit.is_empty() {
            self.draw_text(
                metric.unit,
                metric.bounds.x + metric.bounds.width,
                metric.bounds.y + 32.0,
                16.0,
                style.muted,
                Anchor::Right,
            )?;
        }
        Ok(())
    }
}

fn trip_format_time(duration: f64) -> Result<String> {
    if !duration.is_finite() {
        return Err(Error::Contract("trip duration is not finite"));
    }
    let total = duration
        .round_ties_even()
        .max(0.0)
        .to_u64()
        .ok_or(Error::Contract("trip duration is out of range"))?;
    Ok(format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        total % 3600 / 60,
        total % 60
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trip_duration_rounds_before_carry_and_clamps_negative() {
        for (duration, expected) in [
            (3599.5, "01:00:00"),
            (3661.5, "01:01:02"),
            (3662.5, "01:01:02"),
            (-2.0, "00:00:00"),
        ] {
            assert_eq!(trip_format_time(duration).unwrap(), expected);
        }
        assert!(trip_format_time(f64::INFINITY).is_err());
    }
}
