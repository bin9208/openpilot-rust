use super::{
    helpers::world_label_scale, metric_layout::metric_lines, radar_info_shows_distance,
    radar_info_shows_radar_points, radar_info_shows_speed, radar_point_distance_label,
    radar_point_speed_label,
};
use crate::{
    renderer::{
        tokens::{
            RADAR_LABEL_ANCHOR_Z_OFFSET_M, RADAR_LABEL_DISTANCE_FONT_SIZE,
            RADAR_LABEL_SPEED_FONT_SIZE,
        },
        Anchor, ClusterUiRenderer, Result,
    },
    scene::{CameraSpec, RadarPointMarker, Vec3},
};
use std::time::Instant;
impl ClusterUiRenderer {
    /// # Errors
    /// Returns projection or world label drawing errors.
    pub fn draw_radar_point_labels(
        &mut self,
        points: &[RadarPointMarker],
        camera: &CameraSpec,
        shift: f64,
        info_mode: i64,
    ) -> Result<()> {
        if !radar_info_shows_radar_points(info_mode) {
            return Ok(());
        }
        let theme = self.current_theme();
        let start = self.profile_start();
        let mut ordered: Vec<_> = points.iter().collect();
        ordered.sort_by(|a, b| {
            b.longitudinal_m
                .partial_cmp(&a.longitudinal_m)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    b.lateral_m
                        .abs()
                        .partial_cmp(&a.lateral_m.abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| b.label.cmp(&a.label))
        });
        self.profile_add("draw_scene.radar_labels.sort", start);
        let mut project_ms = 0.;
        let mut layout_ms = 0.;
        let mut text_ms = 0.;
        for point in ordered {
            let start = self.profile_enabled.then(Instant::now);
            let screen = self.world_to_screen_label_anchor(
                Vec3::new(
                    point.center.x + shift,
                    point.center.y,
                    point.center.z + RADAR_LABEL_ANCHOR_Z_OFFSET_M,
                ),
                camera,
            )?;
            if let Some(start) = start {
                project_ms += start.elapsed().as_secs_f64() * 1000.;
            }
            let Some(screen) = screen else {
                continue;
            };
            let start = self.profile_enabled.then(Instant::now);
            let distance = if radar_info_shows_distance(info_mode) {
                radar_point_distance_label(point, self.is_metric)
            } else {
                String::new()
            };
            let speed = if radar_info_shows_speed(info_mode) {
                radar_point_speed_label(point, self.is_metric)
            } else {
                String::new()
            };
            let scale = world_label_scale(point.longitudinal_m);
            let sizes = (
                (RADAR_LABEL_DISTANCE_FONT_SIZE * scale).max(9.),
                (RADAR_LABEL_SPEED_FONT_SIZE * scale).max(8.),
            );
            let lines = metric_lines(distance, speed, screen.x, screen.y, sizes, scale);
            let offset = (1.2 * scale).max(1.);
            if let Some(start) = start {
                layout_ms += start.elapsed().as_secs_f64() * 1000.;
            }
            for line in lines {
                for (x, y, color) in [
                    (line.x + offset, line.y + offset, theme.world_label_shadow),
                    (line.x, line.y, theme.world_label_text),
                ] {
                    let start = self.profile_enabled.then(Instant::now);
                    self.draw_world_label_text(&line.text, x, y, line.size, color, Anchor::Center)?;
                    if let Some(start) = start {
                        text_ms += start.elapsed().as_secs_f64() * 1000.;
                    }
                }
            }
        }
        self.profile_add_elapsed("draw_scene.radar_labels.project", project_ms);
        self.profile_add_elapsed("draw_scene.radar_labels.layout", layout_ms);
        self.profile_add_elapsed("draw_scene.radar_labels.text", text_ms);
        Ok(())
    }
}
