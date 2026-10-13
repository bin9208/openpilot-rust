use super::{visible_runs, CameraOverlayProjection};
use crate::{
    renderer::{ClusterUiRenderer, Point, Result},
    scene::MeshStrip,
};
impl ClusterUiRenderer {
    /// # Errors
    /// Returns retained point buffer allocation, upload or drawing errors.
    pub fn draw_camera_overlay_strip(
        &mut self,
        strip: &MeshStrip,
        projection: &CameraOverlayProjection,
        shift: f64,
    ) -> Result<()> {
        if strip.left.len().min(strip.right.len()) < 2 {
            return Ok(());
        }
        let scratch = &mut self.camera_overlay.scratch;
        let count = scratch.prepare(strip, projection, shift, self.camera_overlay.z_offset_m)?;
        if self.gfx.triangle_strip2d_available() {
            scratch.upload(&mut self.gfx, count)?;
            if let Some(native) = &scratch.native {
                for run in visible_runs(&scratch.visible[..count]) {
                    self.gfx.triangle_strip2d(
                        native,
                        run.start * 2,
                        (run.end - run.start) * 2,
                        strip.color,
                    )?;
                }
            }
        } else {
            for index in 0..count - 1 {
                if scratch.visible[index] == 0 || scratch.visible[index + 1] == 0 {
                    continue;
                }
                let point = |index: usize| {
                    let raw = scratch.points[index];
                    Point::new(f64::from(raw.x), f64::from(raw.y))
                };
                let (left0, right0, left1, right1) = (
                    point(index * 2),
                    point(index * 2 + 1),
                    point((index + 1) * 2),
                    point((index + 1) * 2 + 1),
                );
                self.gfx.triangle(left0, right0, right1, strip.color)?;
                self.gfx.triangle(left0, right1, left1, strip.color)?;
            }
        }
        Ok(())
    }
}
