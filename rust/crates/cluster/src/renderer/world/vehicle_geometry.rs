use crate::{
    renderer::{with_alpha, ClusterUiRenderer, IntoColor, Result},
    scene::{math::clamp, Vec3, VehicleBox},
};
use num_traits::ToPrimitive;
pub(super) fn alpha(base: f64, range: f64, confidence: f64) -> u8 {
    (base + range * clamp(confidence, 0., 1.))
        .to_u8()
        .unwrap_or(0)
}
pub(super) fn corner(vehicle: &VehicleBox, x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(
        vehicle.center.x + vehicle.right_x * x + vehicle.forward_x * y,
        vehicle.center.y + vehicle.right_y * x + vehicle.forward_y * y,
        z,
    )
}
pub(super) fn corners(vehicle: &VehicleBox, width: f64, length: f64, z: f64) -> [Vec3; 4] {
    [
        corner(vehicle, -width, -length, z),
        corner(vehicle, width, -length, z),
        corner(vehicle, width, length, z),
        corner(vehicle, -width, length, z),
    ]
}
impl ClusterUiRenderer {
    /// # Errors
    /// Returns native drawing or coordinate conversion errors.
    pub fn draw_quad(&mut self, points: [Vec3; 4], color: impl IntoColor) -> Result<()> {
        let color = color.rgba();
        self.gfx
            .triangle3d(points[0], points[1], points[2], color)?;
        self.gfx.triangle3d(points[0], points[2], points[3], color)
    }
    /// # Errors
    /// Returns native drawing or coordinate conversion errors.
    pub fn draw_vehicle_shadow(&mut self, vehicle: &VehicleBox) -> Result<()> {
        let points = corners(
            vehicle,
            vehicle.width_m * 0.5 * 1.12,
            vehicle.length_m * 0.5 * 1.08,
            0.018,
        );
        self.draw_quad(points, [0, 0, 0, alpha(18., 34., vehicle.confidence)])
    }
    /// # Errors
    /// Returns native drawing or coordinate conversion errors.
    pub fn draw_vehicle_marker(&mut self, vehicle: &VehicleBox) -> Result<()> {
        let mut marker = vehicle.clone();
        let opacity = alpha(80., 150., vehicle.confidence);
        marker.height_m = (vehicle.height_m * 0.45).max(0.42);
        marker.center.z =
            (vehicle.center.z - vehicle.height_m * 0.5).max(0.) + marker.height_m * 0.5;
        marker.width_m = (vehicle.width_m * 0.68).max(0.55);
        marker.length_m = (vehicle.length_m * 0.64).max(1.05);
        marker.body_color = with_alpha(vehicle.body_color, opacity);
        marker.side_color = with_alpha(vehicle.side_color, opacity);
        marker.rear_color = with_alpha(vehicle.rear_color, opacity);
        marker.top_highlight = with_alpha(vehicle.top_highlight, opacity);
        marker.outline_color = with_alpha(vehicle.outline_color, opacity);
        self.draw_vehicle_box(&marker)
    }
}
