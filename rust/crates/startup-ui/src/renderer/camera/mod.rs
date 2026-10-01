mod shaders;
use super::*;
use crate::{
    camera::{CameraDraw, PlaneFormat, Style},
    camera_bridge::ffi::CameraRect,
};

impl Renderer {
    pub fn camera_shader(&mut self, style: Style, external: bool) -> Result<u32, Error> {
        let compact = matches!(style, Style::Compact { .. });
        let index = usize::from(compact) * 2 + usize::from(external);
        if let Some(shader) = self.camera_shaders[index] {
            return Ok(shader);
        }
        let fragment = shaders::fragment(compact, external);
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
