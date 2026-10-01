pub mod incoming;
pub mod outgoing;

use openpilot_messaging::services::{Service, SERVICES};

pub const SOURCE_PROVENANCE: &str =
    "openpilot/cereal/messaging/{bridge,msgq_to_zmq,bridge_zmq}.cc (MIT)";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bridge transport: {0}")]
    Zmq(#[from] zmq::Error),
    #[error("bridge IPC: {0}")]
    Msgq(#[from] openpilot_msgq::Error),
    #[error("bridge signal: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid socket monitor event")]
    Monitor,
}

pub fn port(endpoint: &str) -> u64 {
    let hash = endpoint
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    8023 + hash % (65535 - 8023)
}

pub fn services(whitelist: Option<&str>) -> Vec<&'static Service> {
    let mut selected: Vec<_> = SERVICES
        .iter()
        .filter(|service| whitelist.is_none_or(|value| value.contains(service.name)))
        .collect();
    selected.sort_unstable_by_key(|service| service.name);
    selected
}
