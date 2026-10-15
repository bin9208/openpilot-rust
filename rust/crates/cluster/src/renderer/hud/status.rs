use super::speed::cruise_set_visible;
use crate::config::{GREEN, WHITE};
use crate::models::ClusterUiState;
use crate::renderer::tokens::{
    EGPU_STATUS_CENTER_X, EGPU_STATUS_FONT_SIZE, EGPU_STATUS_H, EGPU_STATUS_W,
    LFA_LANE_ICON_TOP_OFFSET, LFA_LANE_ICON_WIDTH_SCALE, LFA_STATUS_CENTER_X, LFA_STATUS_ICON_SIZE,
    TOP_ICON_SIZE, TOP_STATUS_CENTER_Y, WIFI_STATUS_CENTER_X, WIFI_STATUS_ICON_SIZE,
};
use crate::renderer::{
    with_alpha, Anchor, ClusterUiRenderer, Point, Rect, Result, StaticImage, TextureView,
};

#[derive(Clone, Copy)]
struct IconPaint {
    center_x: f64,
    bottom: f64,
    width: f64,
    height: f64,
    tint: [u8; 4],
    rotation: f64,
}

impl ClusterUiRenderer {
    /// # Errors
    /// Returns native icon, font, and drawing resource errors.
    pub fn draw_drive_status(&mut self, state: &ClusterUiState) -> Result<()> {
        let gear = state
            .gear_text
            .as_deref()
            .unwrap_or("")
            .trim_matches(openpilot_startup_ui::text::whitespace)
            .to_uppercase();
        if !state.debug_ui_visible
            && gear.is_empty()
            && state.cruise_gap.is_none()
            && !cruise_set_visible(state)
            && state.lfa_active.is_none()
            && !state.egpu_active
            && state.external_compute_label.is_empty()
        {
            return Ok(());
        }
        self.draw_network_status(state, TOP_STATUS_CENTER_Y + WIFI_STATUS_ICON_SIZE * 0.5)?;
        let label = if state.egpu_active {
            "eGPU"
        } else {
            &state.external_compute_label
        };
        self.draw_egpu_status(label)?;
        self.draw_lfa_status_icon(state, TOP_STATUS_CENTER_Y + LFA_STATUS_ICON_SIZE * 0.5)
    }
    fn draw_egpu_status(&mut self, label: &str) -> Result<()> {
        if label.is_empty() {
            return Ok(());
        }
        #[allow(clippy::cast_precision_loss)]
        let width =
            EGPU_STATUS_W.max(label.chars().count() as f64 * EGPU_STATUS_FONT_SIZE * 0.7 + 16.0);
        let rect = Rect::new(
            EGPU_STATUS_CENTER_X - width * 0.5,
            TOP_STATUS_CENTER_Y - EGPU_STATUS_H * 0.5,
            width,
            EGPU_STATUS_H,
        );
        self.gfx.rounded(rect, 0.35, 8, [0, 0, 0, 150])?;
        self.gfx.rounded_lines(rect, 0.35, 8, 2.0, GREEN)?;
        self.draw_text(
            label,
            EGPU_STATUS_CENTER_X,
            TOP_STATUS_CENTER_Y + 1.0,
            EGPU_STATUS_FONT_SIZE,
            GREEN,
            Anchor::Center,
        )
    }
    fn draw_network_status(&mut self, state: &ClusterUiState, bottom: f64) -> Result<()> {
        let color = if state.network_connected {
            WHITE
        } else {
            self.current_theme().muted
        };
        let alpha = if state.network_connected { 255 } else { 130 };
        let paint = IconPaint {
            center_x: WIFI_STATUS_CENTER_X,
            bottom,
            width: WIFI_STATUS_ICON_SIZE,
            height: WIFI_STATUS_ICON_SIZE,
            tint: with_alpha(color, alpha),
            rotation: 0.0,
        };
        self.draw_outlined_icon(self.static_texture(StaticImage::Wifi), paint)
            .map(|_| ())
    }
    fn draw_bottom_icon(&mut self, texture: Option<TextureView>, paint: IconPaint) -> Result<bool> {
        let Some(texture) = texture else {
            return Ok(false);
        };
        self.gfx.texture_pro(
            texture,
            Rect::new(
                0.0,
                0.0,
                f64::from(texture.width),
                f64::from(texture.height),
            ),
            Rect::new(
                paint.center_x,
                paint.bottom - paint.height * 0.5,
                paint.width,
                paint.height,
            ),
            Point::new(paint.width * 0.5, paint.height * 0.5),
            paint.rotation,
            paint.tint,
        )?;
        Ok(true)
    }
    fn draw_outlined_icon(
        &mut self,
        texture: Option<TextureView>,
        paint: IconPaint,
    ) -> Result<bool> {
        if texture.is_none() {
            return Ok(false);
        }
        let alpha = paint.tint[3].clamp(150, 245);
        for (dx, dy) in [
            (-2.0, 0.0),
            (2.0, 0.0),
            (0.0, -2.0),
            (0.0, 2.0),
            (-2.0, -2.0),
            (2.0, -2.0),
            (-2.0, 2.0),
            (2.0, 2.0),
        ] {
            self.draw_bottom_icon(
                texture,
                IconPaint {
                    center_x: paint.center_x + dx,
                    bottom: paint.bottom + dy,
                    tint: [5, 9, 12, alpha],
                    ..paint
                },
            )?;
        }
        self.draw_bottom_icon(texture, paint)
    }
    fn draw_lfa_status_icon(&mut self, state: &ClusterUiState, bottom: f64) -> Result<()> {
        let theme = self.current_theme();
        let active = state.lfa_active.unwrap_or(false);
        let texture = if active {
            self.static_texture(StaticImage::LfaActive)
                .or_else(|| self.static_texture(StaticImage::Lfa))
        } else {
            self.static_texture(StaticImage::Lfa)
        };
        let color = if active { WHITE } else { theme.muted };
        let alpha = if active { 255 } else { 190 };
        let wheel = self.draw_bottom_icon(
            texture,
            IconPaint {
                center_x: LFA_STATUS_CENTER_X,
                bottom,
                width: LFA_STATUS_ICON_SIZE,
                height: LFA_STATUS_ICON_SIZE,
                tint: with_alpha(color, alpha),
                rotation: -state.steering_angle_deg.unwrap_or(0.0),
            },
        )?;
        if state.active_lane_line.unwrap_or(false) {
            self.draw_bottom_icon(
                self.static_texture(StaticImage::LfaLane),
                IconPaint {
                    center_x: LFA_STATUS_CENTER_X,
                    bottom: bottom - LFA_LANE_ICON_TOP_OFFSET,
                    width: LFA_STATUS_ICON_SIZE * LFA_LANE_ICON_WIDTH_SCALE,
                    height: LFA_STATUS_ICON_SIZE,
                    tint: with_alpha(if active { GREEN } else { theme.muted }, alpha),
                    rotation: 0.0,
                },
            )?;
        }
        if wheel {
            return Ok(());
        }
        let outline = if active { GREEN } else { theme.muted };
        let center = Point::new(LFA_STATUS_CENTER_X, bottom - LFA_STATUS_ICON_SIZE * 0.5);
        let scale = TOP_ICON_SIZE / 34.0;
        self.gfx.circle(
            center,
            TOP_ICON_SIZE * 0.5,
            with_alpha(outline, if active { 46 } else { 26 }),
        )?;
        let stroke = with_alpha(outline, 210);
        self.gfx.circle_lines(
            Point::new(center.x.trunc(), center.y.trunc()),
            TOP_ICON_SIZE * 0.5,
            stroke,
        )?;
        self.gfx.circle_lines(
            Point::new(center.x.trunc(), (center.y + 1.0).trunc()),
            TOP_ICON_SIZE * 0.26,
            stroke,
        )?;
        self.gfx.line(
            Point::new(center.x - 7.0 * scale, center.y + 5.0 * scale),
            Point::new(center.x + 7.0 * scale, center.y + 5.0 * scale),
            2.2 * scale,
            stroke,
        )?;
        self.gfx.line(
            Point::new(center.x, center.y + 5.0 * scale),
            Point::new(center.x, center.y + 12.0 * scale),
            2.2 * scale,
            stroke,
        )
    }
}
