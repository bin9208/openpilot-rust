//! Alternating native framebuffer owner without replacing the caller's canvas.
use crate::{draw::Draw, geometry::Rect, Error};
use num_traits::ToPrimitive;
use openpilot_startup_ui::{
    overlay_cache::{Backend, Frame, Layer},
    renderer::RenderTarget,
};
pub struct Control {
    pub index: i64,
    pub enabled: bool,
    pub urgent: bool,
}
struct Canvas<'a> {
    draw: &'a mut dyn Draw,
}
impl Backend for Canvas<'_> {
    type Target = Option<RenderTarget>;
    type Error = Error;
    fn framebuffer(&mut self) -> u32 {
        self.draw.native_renderer().map_or(0, Backend::framebuffer)
    }
    fn flush(&mut self) {
        if let Some(r) = self.draw.native_renderer() {
            Backend::flush(r);
        }
    }
    fn allocate(&mut self, size: (i32, i32)) -> Result<Self::Target, Error> {
        self.draw
            .native_renderer()
            .map(|r| Backend::allocate(r, size))
            .transpose()
    }
    fn complete(&mut self, target: &Self::Target) -> Result<bool, Error> {
        match (self.draw.native_renderer(), target) {
            (Some(r), Some(t)) => Backend::complete(r, t),
            _ => Ok(false),
        }
    }
    fn release(&mut self, target: Self::Target) -> Result<(), Error> {
        if let Some(t) = target {
            let r = self
                .draw
                .native_renderer()
                .ok_or(Error::Contract("overlay renderer disappeared"))?;
            Backend::release(r, t)?;
        }
        Ok(())
    }
    fn bind(&mut self, id: u32) {
        if let Some(r) = self.draw.native_renderer() {
            Backend::bind(r, id);
        }
    }
    fn target_framebuffer(&mut self, target: &Self::Target) -> Result<u32, Error> {
        let r = self
            .draw
            .native_renderer()
            .ok_or(Error::Contract("overlay renderer disappeared"))?;
        Backend::target_framebuffer(
            r,
            target
                .as_ref()
                .ok_or(Error::Contract("overlay target absent"))?,
        )
    }
    fn begin_coverage(&mut self) {
        if let Some(r) = self.draw.native_renderer() {
            Backend::begin_coverage(r);
        }
    }
    fn end_coverage(&mut self) {
        if let Some(r) = self.draw.native_renderer() {
            Backend::end_coverage(r);
        }
    }
    fn composite(&mut self, target: &Self::Target, logical: (f32, f32)) -> Result<(), Error> {
        let r = self
            .draw
            .native_renderer()
            .ok_or(Error::Contract("overlay renderer disappeared"))?;
        Backend::composite(
            r,
            target
                .as_ref()
                .ok_or(Error::Contract("overlay target absent"))?,
            logical,
        )
    }
}
pub struct PaintLayer<K> {
    layer: Layer<Option<RenderTarget>, K>,
}
impl<K: PartialEq> PaintLayer<K> {
    pub fn new(phase: i64) -> Self {
        Self {
            layer: Layer::new(phase),
        }
    }
    pub fn paint(
        &mut self,
        draw: &mut dyn Draw,
        rect: Rect,
        key: K,
        control: Control,
        now: impl FnOnce() -> f64,
        paint: impl FnOnce(&mut dyn Draw) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let (pixels, logical) = if let Some(r) = draw.native_renderer() {
            let logical = r.dimensions();
            let scale = r.config.scale;
            let size = |v: f32| -> Result<i32, Error> {
                let n = (v * scale)
                    .to_i32()
                    .ok_or(Error::Contract("overlay size overflow"))?;
                n.checked_add(n % 2)
                    .ok_or(Error::Contract("overlay size overflow"))
            };
            ((size(logical.0)?, size(logical.1)?), logical)
        } else {
            ((0, 0), (0.0, 0.0))
        };
        self.layer.draw(
            &mut Canvas { draw },
            [rect.x, rect.y, rect.width, rect.height],
            key,
            Frame {
                pixels,
                logical,
                index: control.index,
                enabled: control.enabled,
                urgent: control.urgent,
            },
            now,
            |canvas| paint(canvas.draw),
        )
    }
}
