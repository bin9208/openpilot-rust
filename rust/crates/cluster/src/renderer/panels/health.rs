use super::CardStyle;
use crate::config::{AMBER, BLUE_SOFT, GREEN, RED};
use crate::models::ClusterUiState;
use crate::renderer::system_panel::percent_text;
use crate::renderer::{
    Anchor, ClusterUiRenderer, IntoColor, Point, Rect, Result, Ring, SystemStats,
};
use num_traits::ToPrimitive;

struct Gauge<'a> {
    center: Point,
    label: &'a str,
    value: String,
    percent: Option<f64>,
    color: [u8; 4],
}

impl ClusterUiRenderer {
    pub(super) fn draw_system_health_card(
        &mut self,
        state: &ClusterUiState,
        system_info: &SystemStats,
        bounds: Rect,
        style: CardStyle,
    ) -> Result<()> {
        let Rect {
            x,
            y,
            width,
            height,
        } = bounds;
        self.rounded_rect(
            Rect::new(x, y, width, height),
            10.0,
            style.background,
            Some(style.outline),
            1.2,
        )?;
        self.draw_text(
            self.text("system"),
            x + 18.0,
            y + 31.0,
            22.0,
            style.muted,
            Anchor::Left,
        )?;
        let cpu = state.cpu_usage_percent.or(system_info.cpu_used_percent);
        let memory = state
            .memory_used_percent
            .or(system_info.memory_used_percent);
        let disk = state.disk_used_percent.or(system_info.disk_used_percent);
        let metrics = [
            (
                "CPU",
                percent_text(cpu).trim().to_owned(),
                cpu,
                self.system_metric_color(cpu).rgba(),
            ),
            (
                "TEMP",
                state
                    .cpu_temp_c
                    .map_or_else(|| "--°C".into(), |v| format!("{v:.0}°C")),
                state.cpu_temp_c,
                temperature_color(state.cpu_temp_c, style.muted),
            ),
            (
                "MEM",
                percent_text(memory).trim().to_owned(),
                memory,
                self.system_metric_color(memory).rgba(),
            ),
            (
                "DISK",
                percent_text(disk).trim().to_owned(),
                disk,
                self.system_metric_color(disk).rgba(),
            ),
        ];
        let gauge_width = (width - 32.0 - 8.0) * 0.5;
        for (index, (label, value, percent, color)) in metrics.into_iter().enumerate() {
            let column = if index % 2 == 0 { 0.0 } else { 1.0 };
            let center = Point::new(
                x + 16.0 + gauge_width * 0.5 + column * (gauge_width + 8.0),
                y + if index < 2 { 107.0 } else { 224.0 },
            );
            self.draw_system_gauge(
                &Gauge {
                    center,
                    label,
                    value,
                    percent,
                    color,
                },
                style,
            )?;
        }
        self.gfx.line(
            Point::new(x + 18.0, y + 282.0),
            Point::new(x + width - 18.0, y + 282.0),
            1.0,
            style.outline,
        )?;
        self.draw_text(
            self.text("device_angle"),
            x + 18.0,
            y + 308.0,
            18.0,
            style.muted,
            Anchor::Left,
        )?;
        let angles = state.camera_calibration_euler.and_then(|angles| {
            let (pitch, yaw) = (angles[1].to_degrees(), angles[2].to_degrees());
            (pitch.is_finite() && yaw.is_finite()).then_some((pitch, yaw))
        });
        self.draw_device_angle_indicator(
            Rect::new(x + 14.0, y + 325.0, width - 28.0, 117.0),
            angles,
            style,
        )
    }

    fn draw_system_gauge(&mut self, gauge: &Gauge<'_>, style: CardStyle) -> Result<()> {
        let ring = Ring {
            center: gauge.center,
            inner: 36.0,
            outer: 41.0,
            start: 135.0,
            end: 405.0,
            segments: 24,
            color: style.outline,
        };
        self.gfx.ring(ring)?;
        if let Some(percent) = gauge.percent.filter(|value| value.is_finite()) {
            let ratio = percent.clamp(0.0, 100.0) / 100.0;
            if ratio > 0.0 {
                let end = 135.0 + 270.0 * ratio;
                let segments = (24.0 * ratio)
                    .round_ties_even()
                    .to_i32()
                    .unwrap_or(24)
                    .max(4);
                self.gfx.ring(Ring {
                    end,
                    segments,
                    color: BLUE_SOFT.rgba(),
                    ..ring
                })?;
                for angle in [135.0_f64, end] {
                    let radians = angle.to_radians();
                    self.gfx.circle(
                        Point::new(
                            gauge.center.x + 38.5 * radians.cos(),
                            gauge.center.y + 38.5 * radians.sin(),
                        ),
                        2.5,
                        BLUE_SOFT,
                    )?;
                }
            }
        }
        self.draw_text(
            gauge.label,
            gauge.center.x,
            gauge.center.y - 13.0,
            13.0,
            style.muted,
            Anchor::Center,
        )?;
        self.draw_text(
            &gauge.value,
            gauge.center.x,
            gauge.center.y + 9.0,
            23.0,
            gauge.color,
            Anchor::Center,
        )
    }

    fn draw_device_angle_indicator(
        &mut self,
        bounds: Rect,
        angles: Option<(f64, f64)>,
        style: CardStyle,
    ) -> Result<()> {
        let center = Point::new(bounds.x + 52.0, bounds.y + bounds.height * 0.5);
        self.gfx.circle(center, 31.0, style.target)?;
        self.gfx.ring(Ring {
            center,
            inner: 31.0 - 1.2,
            outer: 31.0,
            start: 0.0,
            end: 360.0,
            segments: 24,
            color: style.outline,
        })?;
        self.gfx.line(
            Point::new(center.x - 25.0, center.y),
            Point::new(center.x + 25.0, center.y),
            1.0,
            style.outline,
        )?;
        self.gfx.line(
            Point::new(center.x, center.y - 25.0),
            Point::new(center.x, center.y + 25.0),
            1.0,
            style.outline,
        )?;
        for (label, point) in [
            ("P-", Point::new(center.x, center.y - 38.0)),
            ("P+", Point::new(center.x, center.y + 38.0)),
            ("Y+", Point::new(center.x - 39.0, center.y)),
            ("Y-", Point::new(center.x + 39.0, center.y)),
        ] {
            self.draw_text(label, point.x, point.y, 9.0, style.muted, Anchor::Center)?;
        }
        self.gfx.circle(center, 2.5, style.muted)?;
        if let Some((pitch, yaw)) = angles {
            let offset = device_angle_offset(pitch, yaw, 31.0, 6.0);
            let marker = Point::new(center.x + offset.x, center.y + offset.y);
            self.gfx.line(center, marker, 2.5, BLUE_SOFT)?;
            self.gfx.circle(marker, 6.0, BLUE_SOFT)?;
            self.gfx.circle(marker, 2.2, style.target)?;
        }
        let pitch = angles.map_or_else(|| "P  --°".into(), |(pitch, _)| format!("P {pitch:+.1}°"));
        let yaw = angles.map_or_else(|| "Y  --°".into(), |(_, yaw)| format!("Y {yaw:+.1}°"));
        self.draw_text(
            &pitch,
            bounds.x + bounds.width * 0.45,
            bounds.y + 36.0,
            22.0,
            style.text,
            Anchor::Left,
        )?;
        self.draw_text(
            &yaw,
            bounds.x + bounds.width * 0.45,
            bounds.y + 69.0,
            22.0,
            style.text,
            Anchor::Left,
        )
    }
}

fn device_angle_offset(pitch: f64, yaw: f64, radius: f64, max_angle: f64) -> Point {
    let scale = radius * 0.68 / max_angle;
    Point::new(
        -yaw.clamp(-max_angle, max_angle) * scale,
        pitch.clamp(-max_angle, max_angle) * scale,
    )
}
fn temperature_color(value: Option<f64>, unavailable: [u8; 4]) -> [u8; 4] {
    match value.filter(|value| value.is_finite()) {
        None => unavailable,
        Some(value) if value >= 90.0 => RED.rgba(),
        Some(value) if value >= 75.0 => AMBER.rgba(),
        Some(_) => GREEN.rgba(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_angles_keep_source_sign_and_saturate_each_axis() {
        let offset = device_angle_offset(12.0, -12.0, 31.0, 6.0);
        assert!((offset.x - 21.08).abs() < 1e-12 && (offset.y - 21.08).abs() < 1e-12);
        let offset = device_angle_offset(-3.0, 1.5, 31.0, 6.0);
        assert!(offset.x < 0.0 && offset.y < 0.0);
        assert_eq!(
            temperature_color(Some(f64::NAN), [1, 2, 3, 4]),
            [1, 2, 3, 4]
        );
        assert_eq!(temperature_color(Some(75.0), [0; 4]), AMBER.rgba());
        assert_eq!(temperature_color(Some(90.0), [0; 4]), RED.rgba());
    }
}
