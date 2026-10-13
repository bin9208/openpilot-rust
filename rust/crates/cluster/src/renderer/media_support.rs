use openpilot_startup_ui::{
    egl::{Context, OwnedImage},
    renderer::ShaderResource,
};
use std::{
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};

#[derive(Default)]
pub(super) struct MediaSupport {
    pub yuv_shader: Option<ShaderResource>,
    pub external_shader: Option<ShaderResource>,
    pub egl: Option<Rc<Context>>,
    pub images: BTreeMap<(u64, u32), OwnedImage>,
    pub image_order: VecDeque<(u64, u32)>,
}
