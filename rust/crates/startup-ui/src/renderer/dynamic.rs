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
