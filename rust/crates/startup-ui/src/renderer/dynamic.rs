//! Dynamic GPU textures release on the owning render thread, including late drops.
use super::*;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
pub struct DynamicTexture {
    id: u32,
    pub width: i32,
    pub height: i32,
    releases: Weak<RefCell<Vec<u32>>>,
}
impl DynamicTexture {
    pub fn id(&self) -> u32 {
        self.id
    }
}
impl Drop for DynamicTexture {
    fn drop(&mut self) {
        if let Some(queue) = self.releases.upgrade() {
            queue.borrow_mut().push(self.id);
        }
    }
}
impl Renderer {
    pub fn dynamic_pixels(
        &mut self,
        width: i32,
        height: i32,
        rgba: &[u8],
    ) -> Result<DynamicTexture, Error> {
        self.release_dynamic_textures();
        let id = self.surface.pin_mut().pixel_texture(width, height, rgba)?;
        Ok(DynamicTexture {
            id,
            width,
            height,
            releases: Rc::downgrade(&self.texture_releases),
        })
    }
    pub fn release_dynamic_textures(&mut self) {
        for id in std::mem::take(&mut *self.texture_releases.borrow_mut()) {
            self.surface.pin_mut().texture_release(id);
        }
    }
}

impl crate::draw::TextureResource for DynamicTexture {
    fn id(&self) -> u32 {
        self.id
    }
    fn dimensions(&self) -> (i32, i32) {
        (self.width, self.height)
    }
}

/// CPU-only image decode. The foreign image stays on this thread; only owned bytes cross workers.
pub struct DecodedImage {
    pub width: i32,
    pub height: i32,
    pub rgba: Vec<u8>,
}
impl DecodedImage {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let mut image = ffi::image(
            path.to_str()
                .ok_or(Error::Contract("image path is not UTF-8"))?,
        )?;
        let rgba = image.pin_mut().rgba()?;
        Ok(Self {
            width: image.width(),
            height: image.height(),
            rgba,
        })
    }
}
impl Renderer {
    pub fn dynamic_image(
        &mut self,
        pixels: crate::draw::PixelBuffer<'_>,
    ) -> Result<DynamicTexture, Error> {
        let texture = self.dynamic_pixels(pixels.dimensions.0, pixels.dimensions.1, pixels.rgba)?;
        self.surface.pin_mut().smooth_texture(texture.id)?;
        Ok(texture)
    }
    pub fn ring(&mut self, ring: crate::draw::Ring) {
        self.surface.pin_mut().ring(ffi::Ring {
            center: ffi::Point {
                x: ring.center.x,
                y: ring.center.y,
            },
            inner: ring.inner,
            outer: ring.outer,
            start: ring.start,
            end: ring.end,
            segments: ring.segments,
            color: ring.color,
        });
    }
}
