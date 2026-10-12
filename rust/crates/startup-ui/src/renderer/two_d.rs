use super::*;
pub use crate::bridge::ffi::Point as RawPoint;
pub struct PointBuffer2D {
    points: cxx::UniquePtr<ffi::Points2D>,
    capacity: usize,
}
impl PointBuffer2D {
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}
impl Renderer {
    pub fn triangle_strip2d_available(&self) -> bool {
        self.surface.strip2d_available()
    }
    pub fn points2d(capacity: usize) -> Result<PointBuffer2D, Error> {
        let native = u32::try_from(capacity)
            .map_err(|_| Error::Contract("2D point capacity exceeds native range"))?;
        Ok(PointBuffer2D {
            points: ffi::points2d(native)?,
            capacity,
        })
    }
    pub fn update_points2d(points: &mut PointBuffer2D, values: &[RawPoint]) -> Result<(), Error> {
        Ok(points.points.pin_mut().update(values)?)
    }
    pub fn triangle_strip2d(
        &mut self,
        points: &PointBuffer2D,
        offset: usize,
        count: usize,
        color: u32,
    ) -> Result<(), Error> {
        let offset = u32::try_from(offset)
            .map_err(|_| Error::Contract("2D point offset exceeds native range"))?;
        let count = u32::try_from(count)
            .map_err(|_| Error::Contract("2D point count exceeds native range"))?;
        Ok(self.surface.pin_mut().strip2d(
            points
                .points
                .as_ref()
                .ok_or(Error::Contract("2D point buffer missing"))?,
            offset,
            count,
            color,
        )?)
    }
}
