use super::*;
use crate::overlay_cache::Backend;
impl Backend for Renderer {
    type Target = RenderTarget;
    type Error = Error;
    fn framebuffer(&mut self) -> u32 {
        self.surface.framebuffer()
    }
    fn flush(&mut self) {
        self.surface.pin_mut().render_flush();
    }
    fn allocate(&mut self, size: (i32, i32)) -> Result<RenderTarget, Error> {
        self.overlay_target(size)
    }
    fn complete(&mut self, target: &RenderTarget) -> Result<bool, Error> {
        self.check_resource(&target.releases)?;
        // target_info rejects zero FBOs; inspect allocation validity through a
        // dedicated bridge method before exposing any draw handle.
        Ok(self.surface.overlay_complete(target.id)?)
    }
    fn release(&mut self, target: RenderTarget) -> Result<(), Error> {
        self.check_resource(&target.releases)?;
        drop(target);
        self.release_resources()
    }
    fn bind(&mut self, framebuffer: u32) {
        self.surface.pin_mut().framebuffer_bind(framebuffer);
    }
    fn target_framebuffer(&mut self, target: &RenderTarget) -> Result<u32, Error> {
        Ok(self.target_info(target)?.framebuffer)
    }
    fn begin_coverage(&mut self) {
        self.surface.pin_mut().coverage_begin();
    }
    fn end_coverage(&mut self) {
        self.end_blend();
    }
    fn composite(&mut self, target: &RenderTarget, logical: (f32, f32)) -> Result<(), Error> {
        self.check_resource(&target.releases)?;
        Ok(self
            .surface
            .pin_mut()
            .overlay_composite(target.id, logical.0, logical.1)?)
    }
}
