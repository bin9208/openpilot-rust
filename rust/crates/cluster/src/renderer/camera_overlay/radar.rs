use super::{projection::source_rect, CameraOverlayProjection};
use crate::{
    config::{RADAR_TO_CAMERA_M, WHITE},
    renderer::{
        display::{display_speed, format_radar_distance, speed_unit},
        with_alpha,
        world::{radar_info_shows_distance, radar_info_shows_radar_points},
        Anchor, ClusterUiRenderer, Rect, Result,
    },
    scene::{RadarPointMarker, Vec3},
};
impl ClusterUiRenderer {
    /// # Errors
    /// Returns marker or world text drawing errors.
    pub fn draw_camera_overlay_radar_point(
        &mut self,
        point: &RadarPointMarker,
        projection: &CameraOverlayProjection,
        shift: f64,
        info_mode: i64,
    ) -> Result<()> {
        let Some(screen) = self.camera_overlay_screen_xy(
            Vec3::new(
                point.center.x,
                point.center.y + RADAR_TO_CAMERA_M,
                point.center.z,
            ),
            projection,
            shift,
        ) else {
            return Ok(());
        };
        let radius = (40. / point.longitudinal_m.abs().max(6.)).clamp(2.5, 5.);
        self.gfx.rectangle(
            source_rect(Rect::new(
                screen.x - radius,
                screen.y - radius,
                radius * 2.,
                radius * 2.,
            )),
            with_alpha(point.color, 225),
        )?;
        if !radar_info_shows_radar_points(info_mode) {
            return Ok(());
        }
        let mut parts = Vec::new();
        if radar_info_shows_distance(info_mode) {
            parts.push(format_radar_distance(point.longitudinal_m, self.is_metric));
        }
        if let Some(speed) = point.absolute_speed_kph {
            parts.push(format!(
                "{:.0} {}",
                display_speed(speed, self.is_metric),
                speed_unit(self.is_metric)
            ));
        }
        if !parts.is_empty() {
            self.draw_world_label_text(
                &parts.join(" "),
                screen.x,
                screen.y - 16.,
                15.,
                with_alpha(WHITE, 220),
                Anchor::Center,
            )?;
        }
        Ok(())
    }
}
