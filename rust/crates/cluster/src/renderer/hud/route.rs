use crate::config::{GREEN, RED, WHITE};
use crate::models::RouteOverlay;
use crate::renderer::tokens::SYSTEM_PANEL_X;
use crate::renderer::{Anchor, ClusterUiRenderer, IntoColor, Point, Rect, Result};
use num_traits::AsPrimitive;

impl ClusterUiRenderer {
    /// # Errors
    /// Returns video upload, font, and native drawing resource errors.
    pub fn draw_route_overlay(&mut self, overlay: Option<&RouteOverlay>) -> Result<()> {
        let Some(overlay) = overlay.filter(|overlay| overlay.panel_visible) else {
            return Ok(());
        };
        let theme = self.current_theme();
        let x = self.information_panel_x(SYSTEM_PANEL_X);
        let stage = self.profile_start();
        self.rounded_rect(
            Rect::new(x, 34.0, 476.0, 410.0),
            18.0,
            theme.route_panel_bg,
            Some(theme.faint.rgba()),
            2.0,
        )?;
        self.profile_add("route_overlay.panel", stage);
        let stage = self.profile_start();
        self.draw_route_video(overlay, Rect::new(x + 10.0, 44.0, 456.0, 244.0))?;
        self.profile_add("route_overlay.video", stage);
        let stage = self.profile_start();
        self.draw_route_data(overlay, Point::new(x + 18.0, 300.0))?;
        self.profile_add("route_overlay.data", stage);
        Ok(())
    }
    fn draw_route_video(&mut self, overlay: &RouteOverlay, bounds: Rect) -> Result<()> {
        let theme = self.current_theme();
        let stage = self.profile_start();
        self.gfx.rounded(bounds, 0.04, 10, theme.route_video_bg)?;
        self.profile_add("route_video.background", stage);
        if overlay.video_rgba.is_none() || overlay.video_width <= 0 || overlay.video_height <= 0 {
            let status = overlay
                .video_status
                .as_deref()
                .filter(|status| !status.is_empty())
                .unwrap_or("qcamera unavailable");
            let stage = self.profile_start();
            self.draw_text(
                status,
                bounds.x + bounds.width * 0.5,
                bounds.y + bounds.height * 0.5,
                20.0,
                theme.route_video_status,
                Anchor::Center,
            )?;
            self.profile_add("route_video.status_text", stage);
            return Ok(());
        }
        let stage = self.profile_start();
        let texture = self.route_video_texture_for_overlay(overlay)?;
        self.profile_add("route_video.texture_for_overlay", stage);
        let Some(texture) = texture else {
            return Ok(());
        };
        let width: f64 = overlay.video_width.as_();
        let height: f64 = overlay.video_height.as_();
        let scale = (bounds.width / width).min(bounds.height / height);
        let target_width = width * scale;
        let target_height = height * scale;
        let stage = self.profile_start();
        self.gfx.texture_pro(
            texture,
            Rect::new(0.0, 0.0, width, height),
            Rect::new(
                bounds.x + (bounds.width - target_width) * 0.5,
                bounds.y + (bounds.height - target_height) * 0.5,
                target_width,
                target_height,
            ),
            Point::default(),
            0.0,
            WHITE,
        )?;
        self.profile_add("route_video.draw_texture", stage);
        Ok(())
    }
    fn draw_route_data(&mut self, overlay: &RouteOverlay, position: Point) -> Result<()> {
        let theme = self.current_theme();
        self.draw_text(
            "CURRENT CODE CUT-IN",
            position.x,
            position.y,
            16.0,
            theme.muted,
            Anchor::Left,
        )?;
        let status = overlay
            .cutin_status
            .as_deref()
            .filter(|status| !status.is_empty())
            .unwrap_or("NEW CUTIN: waiting");
        let (first, second) = status
            .split_once(" | ")
            .map_or((status, None), |(first, second)| (first, Some(second)));
        self.draw_text(
            first,
            position.x,
            position.y + 22.0,
            15.0,
            if first.contains(": YES") { RED } else { GREEN },
            Anchor::Left,
        )?;
        if let Some(second) = second {
            self.draw_text(
                second,
                position.x,
                position.y + 41.0,
                12.0,
                theme.text,
                Anchor::Left,
            )?;
        }
        for (index, line) in (0_u32..).zip(overlay.data_lines.iter().take(6)) {
            self.draw_text(
                line,
                position.x,
                position.y + 61.0 + f64::from(index) * 13.0,
                11.0,
                theme.text,
                Anchor::Left,
            )?;
        }
        Ok(())
    }
}
