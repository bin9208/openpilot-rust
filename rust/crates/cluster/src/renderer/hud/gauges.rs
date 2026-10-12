use crate::config::{
    cluster_camera_view_is_road_camera, AMBER, BLUE, GREEN, MAX_ACCEL_MPS2, RED, WHITE,
};
use crate::models::{ClusterUiState, EnergyGaugeLabel, SteeringOutputKind};
use crate::renderer::tokens::{
    SIDE_GAUGE_BOTTOM, SIDE_GAUGE_COLUMN_GAP, SIDE_GAUGE_LABEL_OFFSET, SIDE_GAUGE_LEFT_CENTER_X,
    SIDE_GAUGE_LOWER_BOTTOM, SIDE_GAUGE_LOWER_TOP, SIDE_GAUGE_OUTLINE, SIDE_GAUGE_TOP,
    SIDE_GAUGE_VALUE_Y, SIDE_GAUGE_WIDTH,
};
use crate::renderer::{Anchor, ClusterUiRenderer, Point, Rect, Result, StrokeStyle};
use crate::scene::math::clamp;

#[derive(Clone, Copy)]
struct GaugeBody {
    center_x: f64,
    top: f64,
    bottom: f64,
    width: f64,
}
impl GaugeBody {
    fn bounds(self) -> Rect {
        Rect::new(
            self.center_x - self.width * 0.5,
            self.top,
            self.width,
            self.bottom - self.top,
        )
    }
}

fn stroke(cache: bool) -> StrokeStyle {
    StrokeStyle {
        color: [5, 9, 12, 255],
        width: 2,
        anchor: Anchor::Center,
        cache,
    }
}

impl ClusterUiRenderer {
    /// # Errors
    /// Returns native drawing and font resource failures.
    pub fn draw_accel_block(&mut self, state: &ClusterUiState) -> Result<()> {
        let theme = self.current_theme();
        let body = GaugeBody {
            center_x: SIDE_GAUGE_LEFT_CENTER_X,
            top: SIDE_GAUGE_TOP,
            bottom: SIDE_GAUGE_BOTTOM,
            width: SIDE_GAUGE_WIDTH,
        };
        let bounds = body.bounds();
        let center = body.top.midpoint(body.bottom).floor();
        let outline = self.side_gauge_outline(state);
        self.rounded_rect(bounds, 18.0, [0, 0, 0, 0], Some(outline), 2.0)?;
        self.gfx.line(
            Point::new(bounds.x, center),
            Point::new(bounds.x + body.width, center),
            3.0,
            theme.gauge_midline,
        )?;
        let value = clamp(state.accel_mps2, -MAX_ACCEL_MPS2, MAX_ACCEL_MPS2);
        let fill = if value > 0.0 {
            GREEN
        } else if value < 0.0 {
            RED
        } else {
            theme.muted
        };
        if value != 0.0 {
            let height =
                (value.abs() / MAX_ACCEL_MPS2 * ((body.bottom - body.top) / 2.0 - 8.0)).trunc();
            let top = if value > 0.0 { center - height } else { center };
            self.rounded_rect(
                Rect::new(bounds.x + 8.0, top, body.width - 16.0, height),
                13.0,
                fill,
                None,
                1.0,
            )?;
        }
        let display = if state.accel_mps2.abs() < 0.005 {
            0.0
        } else {
            state.accel_mps2
        };
        self.draw_text_with_stroke(
            &format!("{display:+05.2}"),
            body.center_x,
            SIDE_GAUGE_VALUE_Y,
            17.0,
            fill,
            stroke(false),
        )?;
        self.draw_text_with_stroke(
            "accel",
            body.center_x,
            body.bottom + SIDE_GAUGE_LABEL_OFFSET,
            17.0,
            WHITE,
            stroke(true),
        )?;
        if let Some(energy) = state.fuel_gauge {
            let label = match state.energy_gauge_label {
                EnergyGaugeLabel::Fuel => "fuel",
                EnergyGaugeLabel::Battery => "battery",
            };
            self.draw_level_gauge(
                GaugeBody {
                    top: SIDE_GAUGE_LOWER_TOP,
                    bottom: SIDE_GAUGE_LOWER_BOTTOM,
                    ..body
                },
                clamp(energy, 0.0, 1.0),
                label,
                if energy > 0.15 { GREEN } else { RED },
                outline,
            )?;
        }
        Ok(())
    }

    /// # Errors
    /// Returns native drawing and font resource failures.
    pub fn draw_steering_output_block(&mut self, state: &ClusterUiState) -> Result<()> {
        let theme = self.current_theme();
        let body = GaugeBody {
            center_x: SIDE_GAUGE_LEFT_CENTER_X + SIDE_GAUGE_COLUMN_GAP,
            top: SIDE_GAUGE_TOP,
            bottom: SIDE_GAUGE_BOTTOM,
            width: SIDE_GAUGE_WIDTH,
        };
        let outline = self.side_gauge_outline(state);
        if let Some(((value, normalized), kind)) = state
            .steering_output
            .zip(state.steering_output_normalized)
            .zip(state.steering_output_kind)
        {
            let normalized = clamp(normalized, -1.0, 1.0);
            let text = if kind == SteeringOutputKind::Angle {
                format!("{value:+.0}%")
            } else if value.abs() > 999.0 {
                format!("{:+.0}%", normalized * 100.0)
            } else {
                format!("{value:+.0}")
            };
            let color = if normalized > 0.0 {
                BLUE
            } else if normalized < 0.0 {
                AMBER
            } else {
                theme.muted
            };
            self.draw_bipolar_gauge(body, normalized, color, outline)?;
            self.draw_text_with_stroke(
                &text,
                body.center_x,
                SIDE_GAUGE_VALUE_Y,
                17.0,
                color,
                stroke(false),
            )?;
            self.draw_text_with_stroke(
                "steer",
                body.center_x,
                body.bottom + SIDE_GAUGE_LABEL_OFFSET,
                17.0,
                WHITE,
                stroke(true),
            )?;
        }
        if let Some(urea) = state.urea_gauge {
            self.draw_level_gauge(
                GaugeBody {
                    top: SIDE_GAUGE_LOWER_TOP,
                    bottom: SIDE_GAUGE_LOWER_BOTTOM,
                    ..body
                },
                clamp(urea, 0.0, 1.0),
                "DEF",
                if urea <= 0.15 { AMBER } else { BLUE },
                outline,
            )?;
        }
        Ok(())
    }

    fn side_gauge_outline(&self, state: &ClusterUiState) -> [u8; 4] {
        if self.current_theme().is_dark
            || cluster_camera_view_is_road_camera(state.camera_view_mode)
        {
            SIDE_GAUGE_OUTLINE
        } else {
            [5, 9, 12, 235]
        }
    }
    fn draw_bipolar_gauge(
        &mut self,
        body: GaugeBody,
        value: f64,
        color: [u8; 3],
        outline: [u8; 4],
    ) -> Result<()> {
        let bounds = body.bounds();
        let center = (body.top + body.bottom) * 0.5;
        self.rounded_rect(bounds, 18.0, [0, 0, 0, 0], Some(outline), 2.0)?;
        self.gfx.line(
            Point::new(bounds.x, center),
            Point::new(bounds.x + body.width, center),
            3.0,
            self.current_theme().gauge_midline,
        )?;
        let height = value.abs() * ((body.bottom - body.top) * 0.5 - 8.0);
        if height <= 0.0 {
            return Ok(());
        }
        self.rounded_rect(
            Rect::new(
                bounds.x + 8.0,
                if value > 0.0 { center - height } else { center },
                body.width - 16.0,
                height,
            ),
            13.0,
            color,
            None,
            1.0,
        )
    }
    fn draw_level_gauge(
        &mut self,
        body: GaugeBody,
        value: f64,
        label: &str,
        color: [u8; 3],
        outline: [u8; 4],
    ) -> Result<()> {
        let bounds = body.bounds();
        self.rounded_rect(bounds, 18.0, [0, 0, 0, 0], Some(outline), 2.0)?;
        let height = value * (body.bottom - body.top - 16.0);
        if height > 0.0 {
            self.rounded_rect(
                Rect::new(
                    bounds.x + 8.0,
                    body.bottom - 8.0 - height,
                    body.width - 16.0,
                    height,
                ),
                13.0,
                color,
                None,
                1.0,
            )?;
        }
        self.draw_text_with_stroke(
            &format!("{:.0}%", value * 100.0),
            body.center_x,
            body.top - 16.0,
            17.0,
            color,
            stroke(false),
        )?;
        self.draw_text_with_stroke(
            label,
            body.center_x,
            body.bottom + SIDE_GAUGE_LABEL_OFFSET,
            17.0,
            WHITE,
            stroke(true),
        )
    }
}
