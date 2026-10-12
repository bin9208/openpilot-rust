use crate::config::GREEN;
use crate::renderer::tokens::{
    LANE_TURN_SIGNAL_CENTER_Y, LANE_TURN_SIGNAL_LEFT_CENTER_X, LANE_TURN_SIGNAL_RIGHT_CENTER_X,
    TURN_SIGNAL_HEAD_HALF_HEIGHT,
};
use crate::renderer::{with_alpha, ClusterUiRenderer, IntoColor, Point, Rect, Result};

#[derive(Clone, Copy)]
pub enum SignalSide {
    Left,
    Right,
}

impl ClusterUiRenderer {
    /// # Errors
    /// Returns native graphics resource failures.
    pub fn draw_turn_signal(
        &mut self,
        side: SignalSide,
        lit: bool,
        show_inactive: bool,
        center_x_offset: f64,
    ) -> Result<()> {
        if !lit && !show_inactive {
            return Ok(());
        }
        let theme = self.current_theme();
        let (base, direction) = match side {
            SignalSide::Left => (LANE_TURN_SIGNAL_LEFT_CENTER_X, -1.0),
            SignalSide::Right => (LANE_TURN_SIGNAL_RIGHT_CENTER_X, 1.0),
        };
        let center = Point::new(base + center_x_offset, LANE_TURN_SIGNAL_CENTER_Y);
        let fill = if lit {
            GREEN.rgba()
        } else {
            with_alpha(theme.muted, 42)
        };
        let outline = if lit {
            [8, 118, 65, 255]
        } else {
            with_alpha(theme.muted, 150)
        };
        let point = |x, y| Point::new(center.x + direction * x, center.y + y);
        let mut tail = Rect::new(
            center.x + direction * (-36.0),
            center.y - 16.0,
            direction * (12.0 - (-36.0)),
            32.0,
        );
        if tail.width < 0.0 {
            tail.x += tail.width;
            tail.width = -tail.width;
        }
        let top = point(12.0, -TURN_SIGNAL_HEAD_HALF_HEIGHT);
        let tip = point(60.0, 0.0);
        let bottom = point(12.0, TURN_SIGNAL_HEAD_HALF_HEIGHT);
        self.gfx.rectangle(tail, fill)?;
        if direction < 0.0 {
            self.gfx.triangle(top, tip, bottom, fill)?;
        } else {
            self.gfx.triangle(top, bottom, tip, fill)?;
        }
        let outline_points = [
            point(-36.0, -16.0),
            point(12.0, -16.0),
            top,
            tip,
            bottom,
            point(12.0, 16.0),
            point(-36.0, 16.0),
        ];
        for (index, start) in outline_points.iter().copied().enumerate() {
            self.gfx.line(
                start,
                outline_points[(index + 1) % outline_points.len()],
                3.0,
                outline,
            )?;
        }
        Ok(())
    }
}
