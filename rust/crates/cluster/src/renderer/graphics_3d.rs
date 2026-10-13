use super::{
    graphics::{color, scalar},
    Graphics, IntoColor, Point, Result,
};
use crate::scene::{CameraSpec, Vec3};
use openpilot_startup_ui::renderer::{
    Camera3D, MeshResource, ModelPaint, ModelResource, PointBuffer3D, Renderer, Vector3,
};

fn vector(value: Vec3) -> Result<Vector3> {
    Ok(Vector3 {
        x: scalar(value.x)?,
        y: scalar(value.y)?,
        z: scalar(value.z)?,
    })
}
fn camera(value: &CameraSpec) -> Result<Camera3D> {
    Ok(Camera3D {
        position: vector(value.position)?,
        target: vector(value.target)?,
        up: Vector3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
        fovy: scalar(value.fovy_deg)?,
        projection: 0,
    })
}

impl Graphics {
    #[must_use]
    pub fn triangle_strip3d_available(&self) -> bool {
        self.native
            .as_ref()
            .is_some_and(Renderer::triangle_strip3d_available)
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn begin_mode3d(&mut self, value: &CameraSpec) -> Result<()> {
        self.backend()?.begin_mode3d(&camera(value)?);
        Ok(())
    }
    /// # Errors
    /// Returns an unopened-surface error.
    pub fn end_mode3d(&mut self) -> Result<()> {
        self.backend()?.end_mode3d();
        Ok(())
    }
    /// # Errors
    /// Returns an unopened-surface error.
    pub fn viewport(&mut self, x: i32, y: i32, width: i32, height: i32) -> Result<()> {
        self.backend()?.viewport([x, y, width, height]);
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn triangle3d(&mut self, a: Vec3, b: Vec3, c: Vec3, tint: impl IntoColor) -> Result<()> {
        self.backend()?
            .triangle3d([vector(a)?, vector(b)?, vector(c)?], color(tint));
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn line3d(&mut self, a: Vec3, b: Vec3, tint: impl IntoColor) -> Result<()> {
        self.backend()?
            .line3d([vector(a)?, vector(b)?], color(tint));
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn sphere(
        &mut self,
        center: Vec3,
        radius: f64,
        rings: i32,
        slices: i32,
        tint: impl IntoColor,
    ) -> Result<()> {
        self.backend()?.sphere(
            vector(center)?,
            scalar(radius)?,
            [rings, slices],
            color(tint),
        );
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn cube_v(&mut self, center: Vec3, size: Vec3, tint: impl IntoColor) -> Result<()> {
        self.backend()?
            .cube_v(vector(center)?, vector(size)?, color(tint));
        Ok(())
    }
    /// # Errors
    /// Returns allocation or invalid native point-buffer errors.
    pub fn points3d(&mut self, points: &[[f32; 3]]) -> Result<PointBuffer3D> {
        self.backend()?;
        Ok(Renderer::points3d(points)?)
    }
    /// # Errors
    /// Returns an unopened-surface or invalid point-buffer error.
    pub fn triangle_strip3d(&mut self, points: &PointBuffer3D, tint: impl IntoColor) -> Result<()> {
        Ok(self.backend()?.triangle_strip3d(points, color(tint))?)
    }
    /// # Errors
    /// Returns camera or coordinate conversion errors.
    pub fn world_to_screen_ex(
        &mut self,
        point: Vec3,
        view: &CameraSpec,
        width: i32,
        height: i32,
    ) -> Result<Point> {
        let value =
            self.backend()?
                .world_to_screen(vector(point)?, &camera(view)?, [width, height]);
        Ok(Point {
            x: f64::from(value.x),
            y: f64::from(value.y),
        })
    }
    /// # Errors
    /// Returns an unopened-surface error.
    pub fn culling(&mut self, enabled: bool) -> Result<()> {
        self.backend()?.culling(enabled);
        Ok(())
    }
    /// # Errors
    /// Returns an unopened-surface error.
    pub fn begin_vertices(&mut self, mode: i32) -> Result<()> {
        self.backend()?.begin_vertices(mode);
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn vertex3(&mut self, point: Vec3) -> Result<()> {
        self.backend()?.vertex3(vector(point)?);
        Ok(())
    }
    /// # Errors
    /// Returns an unopened-surface error.
    pub fn color4(&mut self, tint: impl IntoColor) -> Result<()> {
        self.backend()?.color4(color(tint));
        Ok(())
    }
    /// # Errors
    /// Returns an unopened-surface error.
    pub fn end_vertices(&mut self) -> Result<()> {
        self.backend()?.end_vertices();
        Ok(())
    }
    /// # Errors
    /// Returns an invalid mesh or native model upload error.
    pub fn model(
        &mut self,
        vertices: &[f32],
        normals: &[f32],
        colors: &[u8],
    ) -> Result<ModelResource> {
        Ok(self.backend()?.load_model(vertices, normals, colors)?)
    }
    /// # Errors
    /// Returns allocation or invalid mesh-buffer errors.
    pub fn mesh(
        &mut self,
        vertices: &[f32],
        normals: &[f32],
        colors: &[u8],
    ) -> Result<MeshResource> {
        Ok(self.backend()?.mesh(vertices, normals, colors)?)
    }
    /// # Errors
    /// Returns foreign-owner or native mesh upload errors.
    pub fn upload_mesh(&mut self, mesh: &MeshResource, dynamic: bool) -> Result<()> {
        Ok(self.backend()?.upload_mesh(mesh, dynamic)?)
    }
    /// # Errors
    /// Returns foreign-owner or native model allocation errors.
    pub fn model_from_mesh(&mut self, mesh: MeshResource) -> Result<ModelResource> {
        Ok(self.backend()?.model_from_mesh(mesh)?)
    }
    /// # Errors
    /// Returns coordinate conversion, foreign-owner, or released model errors.
    pub fn draw_model_ex(
        &mut self,
        model: &ModelResource,
        position: Vec3,
        rotation_axis: Vec3,
        angle: f64,
        scale: Vec3,
        tint: impl IntoColor,
    ) -> Result<()> {
        Ok(self.backend()?.model_ex(
            model,
            ModelPaint {
                position: vector(position)?,
                rotation_axis: vector(rotation_axis)?,
                rotation_angle: scalar(angle)?,
                scale: vector(scale)?,
                tint: color(tint),
            },
        )?)
    }
}
