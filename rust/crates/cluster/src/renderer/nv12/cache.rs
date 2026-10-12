use super::super::Result;
use openpilot_startup_ui::renderer::{RenderTarget, Renderer, TextureFilter};

pub(super) struct CachedTarget {
    target: Option<RenderTarget>,
    filter: TextureFilter,
}
impl CachedTarget {
    pub const fn new(filter: TextureFilter) -> Self {
        Self {
            target: None,
            filter,
        }
    }
    pub fn close(&mut self) {
        self.target = None;
    }
    pub fn get(&mut self, backend: &mut Renderer, dimensions: (i32, i32)) -> Result<&RenderTarget> {
        if self
            .target
            .as_ref()
            .is_some_and(|target| (target.width, target.height) != dimensions)
        {
            self.target = None;
        }
        if self.target.is_none() {
            let target = backend.render_target(dimensions)?;
            backend.target_filter(&target, self.filter)?;
            self.target = Some(target);
        }
        self.target
            .as_ref()
            .ok_or(super::super::Error::Contract("NV12 render target missing"))
    }
}
