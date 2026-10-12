use crate::{
    config::{RED, WHITE},
    models::ClusterUiState,
    renderer::{
        tokens::{
            TPMS_LOW_PRESSURE_PSI, TPMS_STATUS_CAR_CENTER_Y, TPMS_STATUS_CAR_H, TPMS_STATUS_CAR_W,
            TPMS_STATUS_CENTER_X, TPMS_STATUS_COLUMN_OFFSET, TPMS_STATUS_FONT_SIZE,
            TPMS_STATUS_ICON_H, TPMS_STATUS_ICON_W, TPMS_STATUS_ROW_OFFSET,
            TPMS_STATUS_VALUE_CENTER_Y, TPMS_STATUS_WHEEL_H, TPMS_STATUS_WHEEL_W,
        },
        Anchor, ClusterUiRenderer, Point, Rect, Result, StrokeStyle,
    },
};
impl ClusterUiRenderer {
    /// # Errors
    /// Returns text, texture or primitive drawing errors.
    pub fn draw_tpms_status(&mut self, state: &ClusterUiState) -> Result<()> {
        let pressures = [state.tpms.fl, state.tpms.fr, state.tpms.rl, state.tpms.rr];
        if pressures.iter().all(Option::is_none) {
            return Ok(());
        }
        if !self.draw_tpms_car_icon()? {
            self.draw_tpms_car_fallback()?;
        }
        for ((x, y), pressure) in tpms_positions().into_iter().zip(pressures) {
            self.draw_compact_tpms_value(pressure, x, y)?;
        }
        Ok(())
    }
    /// # Errors
    /// Returns texture drawing errors.
    pub fn draw_tpms_car_icon(&mut self) -> Result<bool> {
        let Some(texture) = self.tpms_car_texture() else {
            return Ok(false);
        };
        self.gfx.texture_pro(
            texture,
            Rect::new(0., 0., f64::from(texture.width), f64::from(texture.height)),
            Rect::new(
                TPMS_STATUS_CENTER_X - TPMS_STATUS_ICON_W * 0.5,
                TPMS_STATUS_CAR_CENTER_Y - TPMS_STATUS_ICON_H * 0.5,
                TPMS_STATUS_ICON_W,
                TPMS_STATUS_ICON_H,
            ),
            Point::default(),
            0.,
            WHITE,
        )?;
        Ok(true)
    }
    /// # Errors
    /// Returns primitive drawing errors.
    pub fn draw_tpms_car_fallback(&mut self) -> Result<()> {
        let (fill, outline, glass) = if self.current_theme().is_dark {
            ([0, 0, 0, 255], [214, 224, 231, 237], [70, 190, 224, 220])
        } else {
            (
                [245, 248, 252, 255],
                [112, 124, 134, 237],
                [75, 185, 216, 220],
            )
        };
        for (x, y) in tpms_positions() {
            self.rounded_rect(
                Rect::new(
                    x - TPMS_STATUS_WHEEL_W * 0.5,
                    y - TPMS_STATUS_WHEEL_H * 0.5,
                    TPMS_STATUS_WHEEL_W,
                    TPMS_STATUS_WHEEL_H,
                ),
                7.,
                [5, 9, 12, 247],
                Some([180, 192, 201, 235]),
                1.4,
            )?;
        }
        let x = TPMS_STATUS_CENTER_X - TPMS_STATUS_CAR_W * 0.5;
        let y = TPMS_STATUS_CAR_CENTER_Y - TPMS_STATUS_CAR_H * 0.5;
        self.rounded_rect(
            Rect::new(x, y, TPMS_STATUS_CAR_W, TPMS_STATUS_CAR_H),
            6.5,
            fill,
            Some(outline),
            2.,
        )?;
        self.rounded_rect(
            Rect::new(x + 6., y + 10., TPMS_STATUS_CAR_W - 12., 21.),
            5.,
            glass,
            Some(outline),
            1.2,
        )?;
        self.gfx.line(
            Point::new(x + 6., y + 56.),
            Point::new(x + TPMS_STATUS_CAR_W - 6., y + 56.),
            1.5,
            outline,
        )
    }
    /// # Errors
    /// Returns text drawing errors.
    pub fn draw_compact_tpms_value(&mut self, pressure: Option<f64>, x: f64, y: f64) -> Result<()> {
        let text = pressure.map_or_else(|| "--".into(), |value| format!("{value:.0}"));
        let color = pressure.map_or([170, 180, 188, 255], |value| {
            let color = if value < TPMS_LOW_PRESSURE_PSI {
                RED
            } else {
                WHITE
            };
            [color[0], color[1], color[2], 255]
        });
        self.draw_text_with_stroke(
            &text,
            x,
            y,
            TPMS_STATUS_FONT_SIZE,
            color,
            StrokeStyle {
                color: [0, 0, 0, 255],
                width: 2,
                anchor: Anchor::Center,
                cache: false,
            },
        )
    }
}
fn tpms_positions() -> [(f64, f64); 4] {
    [
        (
            TPMS_STATUS_CENTER_X - TPMS_STATUS_COLUMN_OFFSET,
            TPMS_STATUS_VALUE_CENTER_Y - TPMS_STATUS_ROW_OFFSET,
        ),
        (
            TPMS_STATUS_CENTER_X + TPMS_STATUS_COLUMN_OFFSET,
            TPMS_STATUS_VALUE_CENTER_Y - TPMS_STATUS_ROW_OFFSET,
        ),
        (
            TPMS_STATUS_CENTER_X - TPMS_STATUS_COLUMN_OFFSET,
            TPMS_STATUS_VALUE_CENTER_Y + TPMS_STATUS_ROW_OFFSET,
        ),
        (
            TPMS_STATUS_CENTER_X + TPMS_STATUS_COLUMN_OFFSET,
            TPMS_STATUS_VALUE_CENTER_Y + TPMS_STATUS_ROW_OFFSET,
        ),
    ]
}
