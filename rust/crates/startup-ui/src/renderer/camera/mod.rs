mod shaders;
use super::*;
use crate::{
    camera::{CameraDraw, PlaneFormat, Style},
    camera_bridge::ffi::CameraRect,
};

impl Renderer {
    pub fn flush_camera(&mut self) {
        self.surface.pin_mut().flush_camera();
    }
    pub fn release_cluster_camera_shader(&mut self, external: bool) -> Result<(), Error> {
        if let Some(shader) = self.camera_shaders[4 + usize::from(external)].take() {
            self.surface.pin_mut().shader_unload(shader)?;
        }
        Ok(())
    }
    pub fn camera_shader(&mut self, style: Style, external: bool) -> Result<u32, Error> {
        let kind = match style {
            Style::Large => 0,
            Style::Compact { .. } => 1,
            Style::Cluster => 2,
        };
        let index = kind * 2 + usize::from(external);
        if let Some(shader) = self.camera_shaders[index] {
            return Ok(shader);
        }
        let fragment = match style {
            Style::Cluster => shaders::cluster_fragment(external),
            Style::Large => shaders::fragment(false, external),
            Style::Compact { .. } => shaders::fragment(true, external),
        };
        let shader = self
            .surface
            .pin_mut()
            .shader_load(shaders::VERTEX, &fragment)?;
        self.camera_shaders[index] = Some(shader);
        Ok(shader)
    }
    pub fn camera_plane(
        &mut self,
        dimensions: (i32, i32),
        format: PlaneFormat,
    ) -> Result<DynamicTexture, Error> {
        self.release_dynamic_textures();
        let id = self.surface.pin_mut().plane_texture(
            dimensions.0,
            dimensions.1,
            matches!(format, PlaneFormat::Chroma),
        )?;
        Ok(DynamicTexture {
            id,
            width: dimensions.0,
            height: dimensions.1,
            releases: Rc::downgrade(&self.texture_releases),
        })
    }
    pub fn update_camera_plane(&mut self, texture: u32, bytes: &[u8]) -> Result<(), Error> {
        Ok(self.surface.pin_mut().plane_update(texture, bytes)?)
    }
    pub fn smooth_camera_plane(&mut self, texture: u32) -> Result<(), Error> {
        Ok(self.surface.pin_mut().camera_plane_filter(texture)?)
    }
    pub fn native_texture(&self, texture: u32) -> Result<u32, Error> {
        Ok(self.surface.texture_native(texture)?)
    }
    pub fn camera(&mut self, camera: CameraDraw) -> Result<(), Error> {
        let shader = self.camera_shader(camera.style, camera.chroma.is_none())?;
        if let Style::Compact { engaged, driver } = camera.style {
            self.surface
                .pin_mut()
                .uniform_int(shader, "engaged", i32::from(engaged))?;
            self.surface
                .pin_mut()
                .uniform_int(shader, "enhance_driver", i32::from(driver))?;
        }
        let rect = |r: Rect| CameraRect {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        };
        Ok(self.surface.pin_mut().camera_texture(
            shader,
            camera.luma,
            camera.chroma.unwrap_or(0),
            camera.chroma.is_none(),
            rect(camera.source),
            rect(camera.destination),
        )?)
    }
}
