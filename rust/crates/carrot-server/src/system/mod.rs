//! Original features/system.py and services/{device_info,time_sync}.py (#225).
pub mod actions;
pub(crate) mod background;
pub mod calibration;
pub mod defaults;
mod fresh;
pub(crate) mod http;
mod http_read;
mod http_time;
pub mod network;
pub mod time_command;
pub mod time_sync;
mod wifi;

pub struct Service {
    pub network: network::Network,
    pub time_sync: time_sync::TimeSync,
    pub regulatory: std::path::PathBuf,
}

impl Service {
    pub fn original(config: &crate::config::Config) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            network: network::Network::default(),
            time_sync: time_sync::TimeSync::default(),
            regulatory: config.shared_assets.join("offroad/fcc.html"),
        })
    }
}
