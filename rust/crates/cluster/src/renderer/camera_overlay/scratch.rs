use super::{screen_xy, CameraOverlayProjection};
use crate::{
    renderer::{Error, Graphics, PointBuffer2D, RawPoint, Result},
    scene::MeshStrip,
};
use openpilot_ui_application::onroad::model_renderer::math::float;

#[derive(Default)]
pub(super) struct Scratch {
    pub points: Vec<RawPoint>,
    pub visible: Vec<u8>,
    pub native: Option<PointBuffer2D>,
    native_capacity: usize,
}
impl Scratch {
    pub fn capacities(&self) -> (usize, usize, usize) {
        (self.points.len(), self.visible.len(), self.native_capacity)
    }
    pub fn prepare(
        &mut self,
        strip: &MeshStrip,
        view: &CameraOverlayProjection,
        shift: f64,
        z_offset: f64,
    ) -> Result<usize> {
        let count = strip.left.len().min(strip.right.len());
        let point_count = count
            .checked_mul(2)
            .ok_or(Error::Contract("camera overlay point count overflow"))?;
        grow(&mut self.points, point_count)?;
        grow(&mut self.visible, count)?;
        for (index, (left, right)) in strip.left.iter().zip(strip.right.iter()).enumerate() {
            self.visible[index] = 0;
            if let (Some(left), Some(right)) = (
                screen_xy(*left, view, shift + strip.x_offset_m, z_offset),
                screen_xy(*right, view, shift + strip.x_offset_m, z_offset),
            ) {
                self.points[index * 2] = RawPoint {
                    x: float(left.x),
                    y: float(left.y),
                };
                self.points[index * 2 + 1] = RawPoint {
                    x: float(right.x),
                    y: float(right.y),
                };
                self.visible[index] = 1;
            }
        }
        Ok(count)
    }
    pub fn upload(&mut self, gfx: &mut Graphics, count: usize) -> Result<&PointBuffer2D> {
        if self.native_capacity < self.points.len() {
            self.native = Some(gfx.points2d(self.points.len())?);
            self.native_capacity = self.points.len();
        }
        let native = self
            .native
            .as_mut()
            .ok_or(Error::Contract("camera overlay points missing"))?;
        gfx.update_points2d(native, &self.points[..count * 2])?;
        Ok(native)
    }
}
fn grow<T: Default + Clone>(values: &mut Vec<T>, count: usize) -> Result<()> {
    if values.len() < count {
        let capacity = count
            .checked_next_power_of_two()
            .ok_or(Error::Contract("camera overlay capacity overflow"))?;
        values.resize(capacity, T::default());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{models::ClusterUiState, renderer::Rect, scene::Vec3};

    #[test]
    fn buffers_retain_power_of_two_storage_and_clear_rejected_pairs() -> Result<()> {
        let view = super::super::projection(
            &ClusterUiState::default(),
            Rect::new(0., 0., 1124., 480.),
            false,
            0.,
        );
        let mut scratch = Scratch::default();
        let points = vec![Vec3::new(0., 34.18, 0.); 3];
        let strip = MeshStrip::new(points.clone(), points, [10, 20, 30, 255]);
        assert_eq!(scratch.prepare(&strip, &view, 0., 0.)?, 3);
        assert_eq!(scratch.capacities(), (8, 4, 0));
        let pointers = (scratch.points.as_ptr(), scratch.visible.as_ptr());
        let rejected = MeshStrip::new(
            vec![Vec3::new(100., 34.18, 0.); 2],
            vec![Vec3::new(0., 34.18, 0.); 2],
            [10, 20, 30, 255],
        );
        assert_eq!(scratch.prepare(&rejected, &view, 0., 0.)?, 2);
        assert_eq!(
            (scratch.points.as_ptr(), scratch.visible.as_ptr()),
            pointers
        );
        assert_eq!(&scratch.visible[..2], &[0, 0]);
        assert_eq!(scratch.capacities(), (8, 4, 0));
        Ok(())
    }
}
