use crate::{
    renderer::{ClusterUiRenderer, Result},
    scene::{MeshStrip, Vec3},
};
impl ClusterUiRenderer {
    /// # Errors
    /// Returns point conversion, buffer allocation, or drawing errors.
    pub fn draw_strip(&mut self, strip: &MeshStrip) -> Result<()> {
        let count = strip.left.len().min(strip.right.len());
        if count < 2 {
            return Ok(());
        }
        if self.gfx.triangle_strip3d_available() {
            let points = self
                .world
                .strips
                .points_for(strip, |points| self.gfx.points3d(points))?;
            if strip.x_offset_m == 0. {
                self.gfx.triangle_strip3d(points, strip.color)
            } else {
                self.gfx.push_matrix()?;
                let result = self
                    .gfx
                    .translate(strip.x_offset_m, 0., 0.)
                    .and_then(|()| self.gfx.triangle_strip3d(points, strip.color));
                let cleanup = self.gfx.pop_matrix();
                result.and(cleanup)
            }
        } else {
            for index in 0..count - 1 {
                let offset = |point: Vec3| Vec3::new(point.x + strip.x_offset_m, point.y, point.z);
                self.gfx.triangle3d(
                    offset(strip.left[index]),
                    offset(strip.right[index]),
                    offset(strip.right[index + 1]),
                    strip.color,
                )?;
                self.gfx.triangle3d(
                    offset(strip.left[index]),
                    offset(strip.right[index + 1]),
                    offset(strip.left[index + 1]),
                    strip.color,
                )?;
            }
            Ok(())
        }
    }
}
