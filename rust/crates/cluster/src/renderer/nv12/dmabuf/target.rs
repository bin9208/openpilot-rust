use super::{DmaTargetPool, Error, ImportedTarget, PackedLayout, Result};
use openpilot_startup_ui::renderer::TargetInfo;
use std::os::fd::{AsRawFd, BorrowedFd};

impl DmaTargetPool {
    pub(super) fn import_target(
        &mut self,
        fd: BorrowedFd<'_>,
        layout: PackedLayout,
    ) -> Result<ImportedTarget> {
        let fd = fd.try_clone_to_owned().map_err(Error::Duplicate)?;
        let image = self
            .egl
            .import(&layout.attributes(fd.as_raw_fd()))?
            .ok_or_else(|| Error::Import(self.egl.error()))?;
        let previous_texture = u32::from_ne_bytes(self.gl.integer(0x8069).to_ne_bytes());
        let previous_framebuffer = u32::from_ne_bytes(self.gl.integer(0x8CA6).to_ne_bytes());
        self.clear_errors("encoder DMA-BUF import")?;
        let mut texture = 0;
        let mut framebuffer = 0;
        let result = (|| -> Result<()> {
            texture = self.gl.generate_texture()?;
            self.gl.bind_texture(0x0DE1, texture)?;
            image.bind_target(0x0DE1)?;
            for (parameter, value) in [
                (0x2801, 0x2600),
                (0x2800, 0x2600),
                (0x2802, 0x812F),
                (0x2803, 0x812F),
            ] {
                self.gl.texture_parameter(0x0DE1, parameter, value)?;
            }
            framebuffer = self.gl.generate_framebuffer()?;
            self.gl.bind_framebuffer(0x8D40, framebuffer);
            self.gl.attach_texture(0x8D40, 0x8CE0, texture)?;
            let status = self.gl.framebuffer_status(0x8D40)?;
            let error = self.gl.error();
            if status != 0x8CD5 || error != 0 {
                return Err(Error::Framebuffer { status, error });
            }
            Ok(())
        })();
        if result.is_err() {
            if framebuffer != 0 {
                self.gl.delete_framebuffer(framebuffer)?;
            }
            if texture != 0 {
                self.gl.delete_texture(texture)?;
            }
        }
        self.gl.bind_texture(0x0DE1, previous_texture)?;
        self.gl.bind_framebuffer(0x8D40, previous_framebuffer);
        result?;
        Ok(ImportedTarget {
            info: TargetInfo {
                framebuffer,
                texture,
                width: layout.width,
                height: layout.height,
            },
            _image: image,
            _fd: fd,
        })
    }
}
