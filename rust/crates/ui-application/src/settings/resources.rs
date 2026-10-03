use crate::services::egpu::Backend;
use openpilot_ui_framework::{application::TickRegistry, network::WifiSession};
use std::{rc::Rc, sync::Arc};

#[derive(Clone)]
pub struct Network {
    pub session: WifiSession,
    pub params: Rc<openpilot_params::Params>,
}
#[derive(Clone)]
pub struct Resources {
    pub network: Network,
    pub ticks: TickRegistry,
    pub egpu: Arc<dyn Backend>,
}
