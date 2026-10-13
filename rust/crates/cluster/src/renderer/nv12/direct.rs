use super::{plane, portrait, DirectPacking, Nv12Layout, Nv12Packer};
use crate::renderer::{ClusterUiRenderer, Error, Result};
use openpilot_startup_ui::renderer::TargetInfo;

fn packing(layout: Nv12Layout) -> Result<DirectPacking> {
    layout.validate()?;
    layout.direct_packing().ok_or(Error::Contract(
        "direct NV12 readback requires a four-byte packed Venus layout",
    ))
}

impl ClusterUiRenderer {
    /// # Errors
    /// Rejects incompatible packed layouts and capture, GPU packing, or consumer failures.
    pub fn render_to_nv12_target<T>(
        &mut self,
        packer: &mut Nv12Packer,
        layout: Nv12Layout,
        flip_x: bool,
        draw: impl FnOnce(&mut Self) -> Result<()>,
        consume: impl FnOnce(TargetInfo) -> Result<T>,
    ) -> Result<T> {
        let packing = packing(layout)?;
        self.render_to_capture(draw)?;
        let background = Nv12Packer::background(self);
        let backend = self.gfx.backend()?;
        let draw = portrait::prepare(
            backend,
            &mut packer.upload,
            &mut packer.shader,
            layout,
            flip_x,
            background,
        )?;
        let target = packer.full.get(backend, (packing.width, packing.height))?;
        plane::draw(
            backend,
            draw.with_target(
                target,
                (packing.width, layout.y_scanlines),
                0,
                packing.y_y,
                Some([128; 4]),
            ),
        )?;
        plane::draw(
            backend,
            draw.with_target(
                target,
                (packing.width, layout.uv_scanlines),
                1,
                packing.uv_y,
                None,
            ),
        )?;
        consume(backend.target_info(target)?)
    }

    /// # Errors
    /// Rejects incompatible layouts, destination dimensions, or checked external-target drawing failures.
    pub fn render_to_nv12_external(
        &mut self,
        packer: &mut Nv12Packer,
        layout: Nv12Layout,
        flip_x: bool,
        target: TargetInfo,
        draw: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        let packing = packing(layout)?;
        if (target.width, target.height) != (packing.width, packing.height) {
            return Err(Error::Contract(
                "encoder DMA-BUF packed target dimensions changed",
            ));
        }
        self.render_to_capture(draw)?;
        let background = Nv12Packer::background(self);
        let backend = self.gfx.backend()?;
        let draw = portrait::prepare(
            backend,
            &mut packer.upload,
            &mut packer.shader,
            layout,
            flip_x,
            background,
        )?;
        for (size, plane, y, clear) in [
            (
                (packing.width, layout.y_scanlines),
                0,
                packing.y_y,
                Some([128; 4]),
            ),
            ((packing.width, layout.uv_scanlines), 1, packing.uv_y, None),
        ] {
            backend.with_external_target(target, |backend| {
                plane::draw(backend, draw.with_current(size, plane, y, clear))
                    .map_err(|error| openpilot_startup_ui::Error::Io(std::io::Error::other(error)))
            })?;
        }
        Ok(())
    }
}
