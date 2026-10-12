use super::*;
pub struct Camera3D {
    pub position: Vector3,
    pub target: Vector3,
    pub up: Vector3,
    pub fovy: f32,
    pub projection: i32,
}
pub struct PointBuffer3D {
    points: cxx::UniquePtr<ffi::Points3D>,
}
pub(super) fn camera(value: &Camera3D) -> ffi::Camera3D {
    ffi::Camera3D {
        position: vector(value.position),
        target: vector(value.target),
        up: vector(value.up),
        fovy: value.fovy,
        projection: value.projection,
    }
}
impl Renderer {
    pub fn triangle_strip3d_available(&self) -> bool {
        self.surface.strip_available()
    }
    pub fn begin_mode3d(&mut self, value: &Camera3D) {
        self.surface.pin_mut().mode3d_begin(camera(value));
    }
    pub fn end_mode3d(&mut self) {
        self.surface.pin_mut().mode3d_end();
    }
    pub fn viewport(&mut self, value: [i32; 4]) {
        self.surface
            .pin_mut()
            .viewport(value[0], value[1], value[2], value[3]);
    }
    pub fn triangle3d(&mut self, points: [Vector3; 3], color: u32) {
        self.surface.pin_mut().triangle3d(
            vector(points[0]),
            vector(points[1]),
            vector(points[2]),
            color,
        );
    }
    pub fn line3d(&mut self, points: [Vector3; 2], color: u32) {
        self.surface
            .pin_mut()
            .line3d(vector(points[0]), vector(points[1]), color);
    }
    pub fn sphere(&mut self, center: Vector3, radius: f32, detail: [i32; 2], color: u32) {
        self.surface
            .pin_mut()
            .sphere(vector(center), radius, detail[0], detail[1], color);
    }
    pub fn cube_v(&mut self, center: Vector3, size: Vector3, color: u32) {
        self.surface
            .pin_mut()
            .cube_v(vector(center), vector(size), color);
    }
    pub fn points3d(points: &[[f32; 3]]) -> Result<PointBuffer3D, Error> {
        let values: Vec<_> = points.iter().flatten().copied().collect();
        Ok(PointBuffer3D {
            points: ffi::points3d(&values)?,
        })
    }
    pub fn triangle_strip3d(&mut self, points: &PointBuffer3D, color: u32) -> Result<(), Error> {
        Ok(self.surface.pin_mut().strip3d(
            points
                .points
                .as_ref()
                .ok_or(Error::Contract("3D point buffer is empty"))?,
            color,
        )?)
    }
    pub fn world_to_screen(&self, point: Vector3, view: &Camera3D, dimensions: [i32; 2]) -> Point {
        let value =
            self.surface
                .world_to_screen(vector(point), camera(view), dimensions[0], dimensions[1]);
        Point {
            x: value.x,
            y: value.y,
        }
    }
    pub fn culling(&mut self, enabled: bool) {
        self.surface.pin_mut().culling(enabled);
    }
    pub fn begin_vertices(&mut self, mode: i32) {
        self.surface.pin_mut().vertices_begin(mode);
    }
    pub fn vertex3(&mut self, value: Vector3) {
        self.surface.pin_mut().vertex3(vector(value));
    }
    pub fn color4(&mut self, color: u32) {
        self.surface.pin_mut().color4(color);
    }
    pub fn end_vertices(&mut self) {
        self.surface.pin_mut().vertices_end();
    }
}
