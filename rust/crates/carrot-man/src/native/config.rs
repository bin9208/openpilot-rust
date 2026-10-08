use crate::Error;
use std::{net::Ipv4Addr, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub params_root: Option<PathBuf>,
    pub memory_root: PathBuf,
    pub prefix: String,
    pub bind: Ipv4Addr,
    pub udp_port: u16,
    pub tcp_port: u16,
    pub http_port: u16,
    pub route_port: u16,
    pub kisa_port: u16,
    pub command_port: u16,
    pub broadcast_port: u16,
    pub geos_library: Option<PathBuf>,
    pub data_root: PathBuf,
    pub repo_root: PathBuf,
    pub web_settings: PathBuf,
}
impl Config {
    pub fn for_runtime() -> Result<Self, Error> {
        let prefix = std::env::var("OPENPILOT_PREFIX").unwrap_or_else(|_| "d".into());
        let data = PathBuf::from("/data");
        let geos_library = std::env::var_os("CARROT_GEOS_LIBRARY").map(PathBuf::from);
        let carrot_data = std::env::var_os("CARROT_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/data/carrot"));
        Ok(Self {
            params_root: std::env::var_os("PARAMS_ROOT").map(PathBuf::from),
            memory_root: PathBuf::from("/dev/shm/params"),
            prefix,
            bind: Ipv4Addr::UNSPECIFIED,
            udp_port: 7706,
            tcp_port: 7712,
            http_port: 7713,
            route_port: 7709,
            kisa_port: 12345,
            command_port: 7710,
            broadcast_port: 7705,
            geos_library,
            data_root: data,
            repo_root: PathBuf::from("/data/openpilot"),
            web_settings: carrot_data.join("state/web_settings.json"),
        })
    }
}
