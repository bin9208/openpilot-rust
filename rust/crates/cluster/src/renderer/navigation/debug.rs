use super::common::number;
use crate::config::{AMBER, DESIGN_HEIGHT, GREEN, RED, WHITE};
use crate::models::{NaviDebugInfo, NaviGuidanceImage};
use crate::renderer::tokens::{
    NAVI_GUIDANCE_IMAGE_H, NAVI_GUIDANCE_IMAGE_W, NAVI_GUIDANCE_IMAGE_X, NAVI_GUIDANCE_IMAGE_Y,
    SYSTEM_PANEL_W, SYSTEM_PANEL_X, SYSTEM_PANEL_Y,
};
use crate::renderer::{Anchor, ClusterUiRenderer, IntoColor, Point, Rect, Result};

impl ClusterUiRenderer {
    /// # Errors
    /// Returns graphics, font, or media resource failures while drawing this source surface.
    pub fn draw_navi_debug_panel(&mut self, info: Option<&NaviDebugInfo>) -> Result<()> {
        let theme = self.current_theme();
        let x = self.information_panel_x(SYSTEM_PANEL_X);
        let y = SYSTEM_PANEL_Y;
        let w = SYSTEM_PANEL_W;
        let h = (number(DESIGN_HEIGHT) - y - 18.0).min(520.0);
        self.rounded_rect(
            Rect::new(x, y, w, h),
            18.0,
            theme.route_panel_bg,
            Some(theme.faint.rgba()),
            2.0,
        )?;
        let Some(info) = info else {
            self.draw_text(
                "NAVI receiver",
                x + 24.0,
                y + 34.0,
                24.0,
                theme.text,
                Anchor::Left,
            )?;
            self.draw_text(
                "waiting for data",
                x + 24.0,
                y + 76.0,
                22.0,
                theme.muted,
                Anchor::Left,
            )?;
            return self.draw_navi_guidance_image_box(None);
        };
        let severity = info.severity.to_lowercase();
        let color = match severity.as_str() {
            "stop" | "warning" => RED,
            "go" => GREEN,
            "caution" => AMBER,
            _ => theme.text,
        };
        let title = self.ellipsize_text(&info.title, 24.0, w - 48.0)?;
        self.draw_text(&title, x + 24.0, y + 34.0, 24.0, color, Anchor::Left)?;
        if !severity.is_empty() && severity != "normal" {
            let badge = Rect::new(x + w - 92.0 - 24.0, y + 23.0, 92.0, 28.0);
            self.rounded_rect(badge, 10.0, color, None, 0.0)?;
            self.draw_text(
                &severity.to_uppercase(),
                badge.x + badge.width * 0.5,
                badge.y + 6.0,
                15.0,
                WHITE,
                Anchor::Center,
            )?;
        }
        let fallback = ["no navi event".to_owned()];
        let lines = if info.lines.is_empty() {
            fallback.as_slice()
        } else {
            info.lines.as_slice()
        };
        let mut line_y = y + 82.0;
        for line in lines.iter().take(4) {
            let text = self.ellipsize_text(line, 19.0, w - 48.0)?;
            self.draw_text(&text, x + 24.0, line_y, 19.0, theme.text, Anchor::Left)?;
            line_y += 31.0;
        }
        self.draw_navi_guidance_image_box(info.guidance_image.as_ref())
    }

    fn draw_navi_guidance_image_box(&mut self, image: Option<&NaviGuidanceImage>) -> Result<()> {
        let theme = self.current_theme();
        let x = self.information_panel_x(NAVI_GUIDANCE_IMAGE_X);
        let y = NAVI_GUIDANCE_IMAGE_Y;
        let w = NAVI_GUIDANCE_IMAGE_W;
        let h = NAVI_GUIDANCE_IMAGE_H;
        self.rounded_rect(
            Rect::new(x, y, w, h),
            14.0,
            theme.panel_bg,
            Some(theme.faint.rgba()),
            2.0,
        )?;
        self.draw_text(
            "3D GUIDE",
            x + 16.0,
            y + 18.0,
            15.0,
            theme.muted,
            Anchor::Left,
        )?;
        let Some(texture) = self.navi_guidance_texture_for(image)? else {
            return Ok(());
        };
        let texture_w = f64::from(texture.width);
        let texture_h = f64::from(texture.height);
        if texture_w <= 0.0 || texture_h <= 0.0 {
            return Ok(());
        }
        let inner = Rect::new(x + 14.0, y + 40.0, w - 28.0, h - 54.0);
        let scale = (inner.width / texture_w).min(inner.height / texture_h);
        let draw_w = texture_w * scale;
        let draw_h = texture_h * scale;
        let dest = Rect::new(
            inner.x + (inner.width - draw_w) * 0.5,
            inner.y + (inner.height - draw_h) * 0.5,
            draw_w,
            draw_h,
        );
        self.gfx.texture_pro(
            texture,
            Rect::new(0.0, 0.0, texture_w, texture_h),
            dest,
            Point::new(0.0, 0.0),
            0.0,
            WHITE,
        )
    }
}
