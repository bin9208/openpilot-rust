mod cache;
mod direct;
pub mod dmabuf;
mod layout;
mod plane;
mod portrait;
pub mod readback;
mod shaders;

use super::{ClusterUiRenderer, Result};
use cache::CachedTarget;
pub use layout::{DirectPacking, Nv12Layout};
use openpilot_startup_ui::renderer::{ShaderResource, TextureFilter};

pub struct Nv12Request {
    pub layout: Nv12Layout,
    pub buffer: Vec<u8>,
    pub flip_x: bool,
}

pub struct Nv12Packer {
    upload: CachedTarget,
    y: CachedTarget,
    uv: CachedTarget,
    full: CachedTarget,
    shader: Option<ShaderResource>,
}

impl Default for Nv12Packer {
    fn default() -> Self {
        Self {
            upload: CachedTarget::new(TextureFilter::Bilinear),
            y: CachedTarget::new(TextureFilter::Point),
            uv: CachedTarget::new(TextureFilter::Point),
            full: CachedTarget::new(TextureFilter::Point),
            shader: None,
        }
    }
}

impl Nv12Packer {
    pub fn close(&mut self) {
        self.upload.close();
        self.y.close();
        self.uv.close();
        self.full.close();
        self.shader = None;
    }

    fn background(renderer: &ClusterUiRenderer) -> u32 {
        let theme = renderer.current_theme();
        u32::from_le_bytes([theme.bg[0], theme.bg[1], theme.bg[2], 255])
    }

    fn pack(&mut self, renderer: &mut ClusterUiRenderer, request: Nv12Request) -> Result<Vec<u8>> {
        let layout = request.layout;
        let background = Self::background(renderer);
        let backend = renderer.gfx.backend()?;
        let draw = portrait::prepare(
            backend,
            &mut self.upload,
            &mut self.shader,
            layout,
            request.flip_x,
            background,
        )?;
        if let Some(packing) = layout.direct_packing() {
            let full = self.full.get(backend, (packing.width, packing.height))?;
            plane::draw(
                backend,
                draw.with_target(
                    full,
                    (packing.width, layout.y_scanlines),
                    0,
                    packing.y_y,
                    Some([128; 4]),
                ),
            )?;
            plane::draw(
                backend,
                draw.with_target(
                    full,
                    (packing.width, layout.uv_scanlines),
                    1,
                    packing.uv_y,
                    None,
                ),
            )?;
            return Ok(backend.target_pixels(full, false)?);
        }
        let full_stride = layout.stride % 4 == 0;
        let dimensions = if full_stride {
            (
                (layout.stride / 4, layout.y_scanlines),
                (layout.stride / 4, layout.uv_scanlines),
            )
        } else {
            let packed_width = layout.width / 4 + i32::from(layout.width % 4 != 0);
            (
                (packed_width, layout.height),
                (packed_width, layout.height / 2 + layout.height % 2),
            )
        };
        let y = self.y.get(backend, dimensions.0)?;
        let uv = self.uv.get(backend, dimensions.1)?;
        plane::draw(
            backend,
            draw.with_target(y, dimensions.0, 0, 0, Some([0; 4])),
        )?;
        let y_pixels = backend.target_pixels(y, false)?;
        plane::draw(
            backend,
            draw.with_target(uv, dimensions.1, 1, 0, Some([0; 4])),
        )?;
        let uv_pixels = backend.target_pixels(uv, false)?;
        layout.copy_planes(request.buffer, [&y_pixels, &uv_pixels], full_stride)
    }
}

impl ClusterUiRenderer {
    /// # Errors
    /// Rejects inconsistent NV12 layouts or a failed capture, GPU pack, or readback.
    pub fn render_to_nv12_buffer(
        &mut self,
        packer: &mut Nv12Packer,
        request: Nv12Request,
        draw: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<Vec<u8>> {
        request.layout.validate()?;
        self.render_to_capture(draw)?;
        packer.pack(self, request)
    }
}
