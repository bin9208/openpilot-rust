use crate::config::{BLUE_SOFT, WHITE};
use crate::renderer::tokens::{
    NAVI_TRAFFIC_SIGNAL_SIZE, NAVI_TURN_LEFT_TYPES, NAVI_TURN_RIGHT_TYPES,
    NAVI_TURN_ROUNDABOUT_TYPES,
};
use crate::renderer::{Anchor, ClusterUiRenderer, IntoColor, Point, Result, Ring};

impl ClusterUiRenderer {
    pub(in crate::renderer) fn draw_navi_round_line(
        &mut self,
        start: Point,
        end: Point,
        width: f64,
        color: impl IntoColor,
    ) -> Result<()> {
        let color = color.rgba();
        self.gfx.line(start, end, width, color)?;
        self.gfx.circle(start, width * 0.5, color)?;
        self.gfx.circle(end, width * 0.5, color)
    }

    pub(in crate::renderer) fn draw_navi_circle_stroke(
        &mut self,
        center: Point,
        radius: f64,
        width: f64,
        color: impl IntoColor,
    ) -> Result<()> {
        self.gfx.ring(Ring {
            center,
            inner: (radius - width).max(0.0),
            outer: radius,
            start: 0.0,
            end: 360.0,
            segments: 48,
            color: color.rgba(),
        })
    }

    pub(in crate::renderer) fn draw_navi_turn_icon(
        &mut self,
        turn_type: i64,
        cx: f64,
        cy: f64,
        size: f64,
    ) -> Result<()> {
        let theme = self.current_theme();
        let radius = size * 0.5;
        self.gfx
            .circle(Point::new(cx, cy), radius, [24, 88, 148, 235])?;
        self.draw_navi_circle_stroke(Point::new(cx, cy), radius, 2.0, BLUE_SOFT)?;
        let color = if theme.is_dark {
            WHITE
        } else {
            [246, 249, 252]
        };
        let width = (size * 0.075).max(3.0);
        let point = |dx, dy| Point::new(cx + dx * size, cy + dy * size);
        if [153, 154, 249].contains(&turn_type) {
            return self.draw_text(
                "TG",
                cx,
                cy - size * 0.16,
                size * 0.34,
                color,
                Anchor::Center,
            );
        }
        if turn_type == 14 {
            self.gfx.ring(Ring {
                center: Point::new(cx, cy - size * 0.04),
                inner: size * 0.16,
                outer: size * 0.16 + width,
                start: 190.0,
                end: 360.0,
                segments: 30,
                color: color.rgba(),
            })?;
            self.draw_navi_round_line(point(0.20, -0.04), point(0.20, 0.28), width, color)?;
            return self.gfx.triangle(
                point(-0.24, 0.23),
                point(-0.34, 0.08),
                point(-0.12, 0.13),
                color,
            );
        }
        if NAVI_TURN_ROUNDABOUT_TYPES.contains(&turn_type) {
            self.gfx.ring(Ring {
                center: Point::new(cx, cy),
                inner: size * 0.16,
                outer: size * 0.16 + width,
                start: 25.0,
                end: 330.0,
                segments: 36,
                color: color.rgba(),
            })?;
            return self.gfx.triangle(
                point(0.29, -0.20),
                point(0.11, -0.22),
                point(0.23, -0.05),
                color,
            );
        }
        let direction = if NAVI_TURN_LEFT_TYPES.contains(&turn_type) {
            -1.0
        } else if NAVI_TURN_RIGHT_TYPES.contains(&turn_type) {
            1.0
        } else {
            0.0
        };
        if direction == 0.0 {
            self.draw_navi_round_line(point(0.0, 0.28), point(0.0, -0.24), width, color)?;
            return self.gfx.triangle(
                point(0.0, -0.35),
                point(-0.11, -0.19),
                point(0.11, -0.19),
                color,
            );
        }
        self.draw_navi_round_line(point(0.0, 0.28), point(0.0, -0.03), width, color)?;
        self.draw_navi_round_line(
            point(0.0, -0.03),
            point(direction * 0.27, -0.03),
            width,
            color,
        )?;
        self.gfx.triangle(
            point(direction * 0.36, -0.03),
            point(direction * 0.22, -0.15),
            point(direction * 0.22, 0.09),
            color,
        )
    }

    pub(in crate::renderer) fn draw_navi_horizontal_arrow(
        &mut self,
        center: Point,
        color: impl IntoColor,
        right: bool,
    ) -> Result<()> {
        let point = |x, y| slot_point(center, if right { 26.0 - x } else { x }, y);
        let width = (3.0 * NAVI_TRAFFIC_SIGNAL_SIZE / 26.0).max(3.0);
        let color = color.rgba();
        self.draw_navi_round_line(point(8.5, 13.0), point(20.5, 13.0), width, color)?;
        let tip = point(6.418, 13.0);
        self.draw_navi_round_line(point(12.532, 7.0), tip, width, color)?;
        self.draw_navi_round_line(tip, point(12.532, 19.0), width, color)
    }

    pub(in crate::renderer) fn draw_navi_uturn_arrow(
        &mut self,
        center: Point,
        color: impl IntoColor,
    ) -> Result<()> {
        let scale = NAVI_TRAFFIC_SIGNAL_SIZE / 26.0;
        let width = (3.0 * scale).max(3.0);
        let color = color.rgba();
        self.gfx.ring(Ring {
            center: Point::new(center.x, center.y - 3.2 * scale),
            inner: 4.7 * scale,
            outer: 4.7 * scale + width,
            start: 190.0,
            end: 360.0,
            segments: 28,
            color,
        })?;
        self.draw_navi_round_line(
            slot_point(center, 17.8, 10.0),
            slot_point(center, 17.8, 18.6),
            width,
            color,
        )?;
        self.gfx.triangle(
            slot_point(center, 4.9, 12.7),
            slot_point(center, 8.0, 18.8),
            slot_point(center, 12.6, 15.0),
            color,
        )
    }
}

fn slot_point(center: Point, x: f64, y: f64) -> Point {
    let scale = NAVI_TRAFFIC_SIGNAL_SIZE / 26.0;
    Point::new(center.x + (x - 13.0) * scale, center.y + (y - 13.0) * scale)
}
