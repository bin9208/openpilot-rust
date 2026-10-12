use super::vehicle_geometry::{alpha, corners};
use crate::{
    config::{AMBER, WHITE},
    renderer::{with_alpha, ClusterUiRenderer, Result},
    scene::{RadarPointMarker, Vec3, VehicleBox},
};
const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];
impl ClusterUiRenderer {
    /// # Errors
    /// Returns native drawing or coordinate conversion errors.
    pub fn draw_vehicle(&mut self, vehicle: &VehicleBox) -> Result<()> {
        let marker = vehicle.source.starts_with("modelV2")
            || matches!(
                vehicle.source.as_str(),
                "radarState" | "radarPoint" | "cornerRadar"
            );
        if self.world.model.is_some()
            && !marker
            && (vehicle.source.is_empty() || vehicle.primary || vehicle.cut_in)
        {
            self.draw_vehicle_shadow(vehicle)?;
            self.draw_vehicle_model(vehicle)
        } else if !vehicle.source.is_empty() && (marker || !vehicle.primary && !vehicle.cut_in) {
            self.draw_vehicle_marker(vehicle)
        } else {
            self.draw_vehicle_box(vehicle)
        }
    }
    /// # Errors
    /// Returns native drawing or coordinate conversion errors; restores culling.
    pub fn draw_vehicle_model(&mut self, vehicle: &VehicleBox) -> Result<()> {
        let Some(model) = self.world.model.as_ref() else {
            return Ok(());
        };
        let tint = if vehicle.source == "radarPoint" {
            vehicle.body_color
        } else {
            with_alpha(WHITE, alpha(92., 163., vehicle.confidence))
        };
        self.gfx.culling(false)?;
        let result = self.gfx.draw_model_ex(
            model,
            Vec3::new(vehicle.center.x, vehicle.center.y, 0.035),
            Vec3::new(0., 0., 1.),
            (-vehicle.forward_x).atan2(vehicle.forward_y).to_degrees(),
            Vec3::new(vehicle.width_m, vehicle.length_m, vehicle.height_m),
            tint,
        );
        let cleanup = self.gfx.culling(true);
        result.and(cleanup)
    }
    /// # Errors
    /// Returns native drawing or coordinate conversion errors.
    pub fn draw_radar_point(&mut self, point: &RadarPointMarker) -> Result<()> {
        let side = (point.radius_m * 1.75).max(0.16);
        self.gfx.cube_v(
            point.center,
            Vec3::new(side, side, (point.radius_m * 1.15).max(0.12)),
            point.color,
        )
    }
    /// # Errors
    /// Returns native drawing or coordinate conversion errors.
    pub fn draw_vehicle_box(&mut self, vehicle: &VehicleBox) -> Result<()> {
        let width = vehicle.width_m * 0.5;
        let length = vehicle.length_m * 0.5;
        let base = corners(vehicle, width, length, 0.035);
        let top = corners(vehicle, width, length, vehicle.height_m + 0.035);
        self.draw_vehicle_shadow(vehicle)?;
        for (points, color) in [
            ([base[0], base[1], top[1], top[0]], vehicle.rear_color),
            ([base[1], base[2], top[2], top[1]], vehicle.side_color),
            ([base[2], base[3], top[3], top[2]], vehicle.body_color),
            ([base[3], base[0], top[0], top[3]], vehicle.side_color),
            (top, vehicle.body_color),
        ] {
            self.draw_quad(points, color)?;
        }
        let highlight = top.map(|point| {
            Vec3::new(
                point.x + (vehicle.center.x - point.x) * 0.22,
                point.y + (vehicle.center.y - point.y) * 0.22,
                point.z + 0.006,
            )
        });
        self.draw_quad(highlight, vehicle.top_highlight)?;
        let points = [
            base[0], base[1], base[2], base[3], top[0], top[1], top[2], top[3],
        ];
        for (a, b) in EDGES {
            self.gfx
                .line3d(points[a], points[b], vehicle.outline_color)?;
        }
        if vehicle.primary || vehicle.cut_in {
            self.draw_selected_vehicle_outline(vehicle)?;
        }
        Ok(())
    }
    /// # Errors
    /// Returns native drawing or coordinate conversion errors.
    pub fn draw_selected_vehicle_outline(&mut self, vehicle: &VehicleBox) -> Result<()> {
        let width = vehicle.width_m * 0.5 + 0.12;
        let length = vehicle.length_m * 0.5 + 0.16;
        let base = corners(vehicle, width, length, 0.035 + 0.015);
        let top = corners(vehicle, width, length, vehicle.height_m + 0.035 + 0.065);
        let points = [
            top[0], top[1], top[2], top[3], base[0], base[1], base[2], base[3],
        ];
        let color = if vehicle.cut_in { AMBER } else { WHITE };
        for (a, b) in &EDGES[..8] {
            self.gfx.line3d(points[*a], points[*b], color)?;
        }
        for index in 0..4 {
            self.gfx.line3d(base[index], top[index], color)?;
        }
        Ok(())
    }
}
