use super::*;
use std::{cell::RefCell, rc::Weak};

#[derive(Clone, Copy)]
pub(super) enum Release {
    Shader(u32),
    Target(u32),
    Model(u32),
    Mesh(u32),
}
type Queue = Weak<RefCell<Vec<Release>>>;
pub struct ShaderResource {
    id: u32,
    releases: Queue,
}
pub struct ModelResource {
    id: u32,
    releases: Queue,
}
pub struct MeshResource {
    id: u32,
    releases: Queue,
}
pub struct RenderTarget {
    pub(super) id: u32,
    pub width: i32,
    pub height: i32,
    pub(super) releases: Queue,
}
struct ExternalTargetFrame<'a>(&'a mut Renderer);
impl Drop for ExternalTargetFrame<'_> {
    fn drop(&mut self) {
        self.0.surface.pin_mut().target_end();
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TargetInfo {
    pub framebuffer: u32,
    pub texture: u32,
    pub width: i32,
    pub height: i32,
}

macro_rules! drop_resource {
    ($name:ident, $kind:ident) => {
        impl Drop for $name {
            fn drop(&mut self) {
                if let Some(queue) = self.releases.upgrade() {
                    queue.borrow_mut().push(Release::$kind(self.id));
                }
            }
        }
    };
}
drop_resource!(ShaderResource, Shader);
drop_resource!(ModelResource, Model);
drop_resource!(MeshResource, Mesh);
drop_resource!(RenderTarget, Target);

impl Renderer {
    fn resource_queue(&self) -> Queue {
        Rc::downgrade(&self.resource_releases)
    }
    pub(super) fn check_resource(&self, owner: &Queue) -> Result<(), Error> {
        if Weak::ptr_eq(owner, &self.resource_queue()) {
            Ok(())
        } else {
            Err(Error::Contract(
                "graphics resource belongs to another surface",
            ))
        }
    }
    pub fn release_resources(&mut self) -> Result<(), Error> {
        for release in std::mem::take(&mut *self.resource_releases.borrow_mut()) {
            match release {
                Release::Shader(id) => self.surface.pin_mut().shader_unload(id)?,
                Release::Target(id) => self.surface.pin_mut().target_release(id)?,
                Release::Model(id) => self.surface.pin_mut().model_release(id)?,
                Release::Mesh(id) => self.surface.pin_mut().mesh_release(id)?,
            }
        }
        Ok(())
    }
    pub fn load_shader(&mut self, vertex: &str, fragment: &str) -> Result<ShaderResource, Error> {
        self.release_resources()?;
        let id = self.surface.pin_mut().shader_load(vertex, fragment)?;
        Ok(ShaderResource {
            id,
            releases: self.resource_queue(),
        })
    }
    pub fn shader_vec2(
        &mut self,
        shader: &ShaderResource,
        name: &str,
        values: [f32; 2],
    ) -> Result<(), Error> {
        self.check_resource(&shader.releases)?;
        Ok(self
            .surface
            .pin_mut()
            .uniform_floats(shader.id, name, &values, 1, 1)?)
    }
    pub fn shader_integer(
        &mut self,
        shader: &ShaderResource,
        name: &str,
        value: i32,
    ) -> Result<(), Error> {
        self.check_resource(&shader.releases)?;
        Ok(self.surface.pin_mut().uniform_int(shader.id, name, value)?)
    }
    pub fn shader_texture(
        &mut self,
        shader: &ShaderResource,
        name: &str,
        texture: &DynamicTexture,
    ) -> Result<(), Error> {
        self.check_resource(&shader.releases)?;
        if !Weak::ptr_eq(&texture.releases, &Rc::downgrade(&self.texture_releases)) {
            return Err(Error::Contract("texture belongs to another surface"));
        }
        Ok(self
            .surface
            .pin_mut()
            .uniform_texture(shader.id, name, texture.id)?)
    }
    pub fn begin_shader(&mut self, shader: &ShaderResource) -> Result<(), Error> {
        self.check_resource(&shader.releases)?;
        Ok(self.surface.pin_mut().shader_begin(shader.id)?)
    }
    pub fn end_shader(&mut self) {
        self.surface.pin_mut().shader_end();
    }
    pub fn custom_blend(&mut self, source: i32, destination: i32, equation: i32) {
        self.surface
            .pin_mut()
            .blend_custom(source, destination, equation);
    }
    pub fn end_blend(&mut self) {
        self.surface.pin_mut().blend_end();
    }
    pub fn render_target(&mut self, dimensions: (i32, i32)) -> Result<RenderTarget, Error> {
        self.release_resources()?;
        let id = self
            .surface
            .pin_mut()
            .target_create(dimensions.0, dimensions.1)?;
        Ok(RenderTarget {
            id,
            width: dimensions.0,
            height: dimensions.1,
            releases: self.resource_queue(),
        })
    }
    pub(super) fn overlay_target(&mut self, dimensions: (i32, i32)) -> Result<RenderTarget, Error> {
        self.release_resources()?;
        let id = self
            .surface
            .pin_mut()
            .overlay_create(dimensions.0, dimensions.1)?;
        Ok(RenderTarget {
            id,
            width: dimensions.0,
            height: dimensions.1,
            releases: self.resource_queue(),
        })
    }
    pub fn target_info(&self, target: &RenderTarget) -> Result<TargetInfo, Error> {
        self.check_resource(&target.releases)?;
        let value = self.surface.target_info(target.id)?;
        Ok(TargetInfo {
            framebuffer: value.framebuffer,
            texture: value.texture,
            width: value.width,
            height: value.height,
        })
    }
    pub fn target_filter(
        &mut self,
        target: &RenderTarget,
        filter: TextureFilter,
    ) -> Result<(), Error> {
        self.check_resource(&target.releases)?;
        Ok(self
            .surface
            .pin_mut()
            .target_filter(target.id, filter as i32)?)
    }
    pub fn begin_target(&mut self, target: &RenderTarget) -> Result<(), Error> {
        self.check_resource(&target.releases)?;
        self.release_resources()?;
        Ok(self.surface.pin_mut().target_begin(target.id)?)
    }
    pub fn end_target(&mut self) {
        self.surface.pin_mut().target_end();
    }
    /// # Errors
    /// Rejects invalid target metadata or callback errors; always ends texture mode.
    pub fn with_external_target<T>(
        &mut self,
        info: TargetInfo,
        draw: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.release_dynamic_textures();
        self.release_resources()?;
        self.surface
            .pin_mut()
            .external_target_begin(ffi::TargetInfo {
                framebuffer: info.framebuffer,
                texture: info.texture,
                width: info.width,
                height: info.height,
            })?;
        let frame = ExternalTargetFrame(self);
        draw(frame.0)
    }
    pub fn target_pixels(
        &self,
        target: &RenderTarget,
        flip_vertical: bool,
    ) -> Result<Vec<u8>, Error> {
        self.check_resource(&target.releases)?;
        Ok(self.surface.target_pixels(target.id, flip_vertical)?)
    }
    pub fn target_texture(
        &mut self,
        target: &RenderTarget,
        paint: TargetPaint,
    ) -> Result<(), Error> {
        self.check_resource(&target.releases)?;
        Ok(self
            .surface
            .pin_mut()
            .target_draw(target.id, texture_paint(paint))?)
    }
    pub fn load_model(
        &mut self,
        vertices: &[f32],
        normals: &[f32],
        colors: &[u8],
    ) -> Result<ModelResource, Error> {
        let mesh = self.mesh(vertices, normals, colors)?;
        self.upload_mesh(&mesh, false)?;
        self.model_from_mesh(mesh)
    }
    pub fn mesh(
        &mut self,
        vertices: &[f32],
        normals: &[f32],
        colors: &[u8],
    ) -> Result<MeshResource, Error> {
        self.release_resources()?;
        let id = self
            .surface
            .pin_mut()
            .mesh_create(vertices, normals, colors)?;
        Ok(MeshResource {
            id,
            releases: self.resource_queue(),
        })
    }
    pub fn upload_mesh(&mut self, mesh: &MeshResource, dynamic: bool) -> Result<(), Error> {
        self.check_resource(&mesh.releases)?;
        Ok(self.surface.pin_mut().mesh_upload(mesh.id, dynamic)?)
    }
    pub fn model_from_mesh(&mut self, mesh: MeshResource) -> Result<ModelResource, Error> {
        self.check_resource(&mesh.releases)?;
        let id = self.surface.pin_mut().model_from_mesh(mesh.id)?;
        Ok(ModelResource {
            id,
            releases: self.resource_queue(),
        })
    }
    pub fn model_ex(&mut self, model: &ModelResource, paint: ModelPaint) -> Result<(), Error> {
        self.check_resource(&model.releases)?;
        Ok(self.surface.pin_mut().model_draw(
            model.id,
            ffi::ModelPaint {
                position: vector(paint.position),
                rotation_axis: vector(paint.rotation_axis),
                rotation_angle: paint.rotation_angle,
                scale: vector(paint.scale),
                tint: paint.tint,
            },
        )?)
    }
}
pub struct TargetPaint {
    pub source: Rect,
    pub destination: Rect,
    pub origin: Point,
    pub rotation: f32,
    pub tint: u32,
}
pub(super) fn texture_paint(paint: TargetPaint) -> ffi::TexturePaint {
    ffi::TexturePaint {
        source: convert(paint.source),
        destination: convert(paint.destination),
        origin: ffi::Point {
            x: paint.origin.x,
            y: paint.origin.y,
        },
        rotation: paint.rotation,
        tint: paint.tint,
    }
}
pub struct ModelPaint {
    pub position: Vector3,
    pub rotation_axis: Vector3,
    pub rotation_angle: f32,
    pub scale: Vector3,
    pub tint: u32,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
pub(super) fn vector(value: Vector3) -> ffi::Vector3 {
    ffi::Vector3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}
