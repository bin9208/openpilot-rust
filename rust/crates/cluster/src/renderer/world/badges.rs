use super::{
    helpers::{vehicle_metric_color, world_label_scale},
    metric_layout::{metric_lines, MetricLabel},
    radar_info_shows_distance, radar_info_shows_speed, radar_info_shows_vehicle,
    vehicle_distance_label, vehicle_distance_m, vehicle_has_lead_one_metrics, vehicle_speed_label,
};
use crate::{
    config::EGO_FORWARD_M,
    renderer::{
        tokens::{
            LEAD_ONE_DISTANCE_FONT_SIZE, LEAD_ONE_SPEED_FONT_SIZE, VEHICLE_BADGE_ANCHOR_Z_OFFSET_M,
            VEHICLE_BADGE_DISTANCE_FONT_SIZE, VEHICLE_BADGE_SPEED_FONT_SIZE,
        },
        ClusterUiRenderer, Result,
    },
    scene::{CameraSpec, Vec3, VehicleBox},
};
use std::time::Instant;
impl ClusterUiRenderer {
    /// # Errors
    /// Returns projection, measurement, or world label drawing errors.
    pub fn draw_vehicle_badges(
        &mut self,
        vehicles: &[VehicleBox],
        camera: &CameraSpec,
        shift: f64,
        info_mode: i64,
        color_mode: i64,
    ) -> Result<()> {
        let show = radar_info_shows_vehicle(info_mode);
        if !show
            && !vehicles
                .iter()
                .any(|vehicle| vehicle.primary || vehicle.cut_in)
        {
            return Ok(());
        }
        let theme = self.current_theme();
        let start = self.profile_start();
        let mut ordered: Vec<_> = vehicles
            .iter()
            .filter(|vehicle| {
                !vehicle.label.is_empty() && (show || vehicle.primary || vehicle.cut_in)
            })
            .collect();
        ordered.sort_by(|a, b| {
            let priority = |vehicle: &VehicleBox| {
                if vehicle.primary {
                    0
                } else if vehicle.cut_in {
                    1
                } else {
                    2
                }
            };
            priority(a)
                .cmp(&priority(b))
                .then_with(|| {
                    (a.center.y - EGO_FORWARD_M)
                        .max(0.)
                        .partial_cmp(&(b.center.y - EGO_FORWARD_M).max(0.))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    (-a.confidence)
                        .partial_cmp(&(-b.confidence))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        self.profile_add("draw_scene.vehicle_badges.sort", start);
        let mut project_ms = 0.;
        let mut layout_ms = 0.;
        let mut labels = Vec::new();
        for vehicle in ordered {
            let start = self.profile_enabled.then(Instant::now);
            let screen = self.world_to_screen_label_anchor(
                Vec3::new(
                    vehicle.center.x + shift,
                    vehicle.center.y,
                    vehicle.height_m + VEHICLE_BADGE_ANCHOR_Z_OFFSET_M,
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
            if let Some(label) = badge_label(
                vehicle,
                screen,
                &theme,
                self.is_metric,
                info_mode,
                color_mode,
            ) {
                labels.push(label);
            }
            if let Some(start) = start {
                layout_ms += start.elapsed().as_secs_f64() * 1000.;
            }
        }
        let start = self.profile_start();
        self.draw_vehicle_metric_labels(&labels, None)?;
        self.profile_add("draw_scene.vehicle_badges.text", start);
        self.profile_add_elapsed("draw_scene.vehicle_badges.project", project_ms);
        self.profile_add_elapsed("draw_scene.vehicle_badges.layout", layout_ms);
        Ok(())
    }
}

fn badge_label<'a>(
    vehicle: &'a VehicleBox,
    screen: crate::renderer::Point,
    theme: &crate::config::ClusterTheme,
    metric: bool,
    info_mode: i64,
    color_mode: i64,
) -> Option<MetricLabel<'a>> {
    let important = vehicle.primary || vehicle.cut_in;
    let distance = if radar_info_shows_distance(info_mode) || important {
        vehicle_distance_label(vehicle, metric)
    } else {
        String::new()
    };
    let speed = if radar_info_shows_speed(info_mode) {
        vehicle_speed_label(vehicle, metric)
    } else {
        String::new()
    };
    if !distance.is_empty() || !speed.is_empty() {
        let lead = vehicle_has_lead_one_metrics(vehicle);
        let scale = if lead {
            1.
        } else {
            world_label_scale(vehicle_distance_m(vehicle))
        };
        let distance_size = if lead {
            LEAD_ONE_DISTANCE_FONT_SIZE
        } else {
            (VEHICLE_BADGE_DISTANCE_FONT_SIZE * scale).max(9.)
        };
        let speed_size = if lead {
            LEAD_ONE_SPEED_FONT_SIZE
        } else {
            (VEHICLE_BADGE_SPEED_FONT_SIZE * scale).max(8.)
        };
        return Some(MetricLabel {
            vehicle,
            lines: metric_lines(
                distance,
                speed,
                screen.x,
                screen.y,
                (distance_size, speed_size),
                scale,
            ),
            color: super::super::with_alpha(vehicle_metric_color(vehicle, theme, color_mode), 255),
            shadow_offset: (1.2 * scale).max(1.),
        });
    }
    None
}
