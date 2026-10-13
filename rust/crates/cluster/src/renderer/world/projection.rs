use super::helpers::camera_depth_m;
use crate::{
    renderer::{graphics::scalar, ClusterUiRenderer, Error, Point, Result},
    scene::{CameraSpec, Vec3},
};
use num_traits::ToPrimitive;
fn float_point(value: Vec3) -> Result<Vec3> {
    Ok(Vec3::new(
        f64::from(scalar(value.x)?),
        f64::from(scalar(value.y)?),
        f64::from(scalar(value.z)?),
    ))
}
impl ClusterUiRenderer {
    /// # Errors
    /// Returns coordinate conversion or graphics errors.
    pub fn world_to_screen_label_anchor(
        &mut self,
        point: Vec3,
        camera: &CameraSpec,
    ) -> Result<Option<Point>> {
        // The source helper receives raylib Vector3/Camera3D float fields.
        let point = float_point(point)?;
        let camera = CameraSpec {
            position: float_point(camera.position)?,
            target: float_point(camera.target)?,
            fovy_deg: f64::from(scalar(camera.fovy_deg)?),
        };
        if !camera_depth_m(point, &camera).is_some_and(|depth| depth > 0.05) {
            return Ok(None);
        }
        let width = self
            .width
            .to_i32()
            .ok_or(Error::Contract("world viewport width invalid"))?;
        let height = self
            .height
            .to_i32()
            .ok_or(Error::Contract("world viewport height invalid"))?;
        let screen = self.gfx.world_to_screen_ex(point, &camera, width, height)?;
        Ok((screen.x.is_finite() && screen.y.is_finite()).then_some(screen))
    }
}
