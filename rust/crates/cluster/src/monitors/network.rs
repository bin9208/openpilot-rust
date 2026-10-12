use super::RefreshWorker;
use openpilot_params::Params;
use std::{
    io,
    net::{IpAddr, UdpSocket},
};

#[must_use]
pub fn valid_address(value: &[u8]) -> Option<String> {
    let value = String::from_utf8_lossy(value);
    let value = value.trim_matches(openpilot_startup_ui::text::whitespace);
    let (address, scope) = value
        .split_once('%')
        .map_or((value, None), |(address, scope)| (address, Some(scope)));
    if scope.is_some_and(|scope| scope.is_empty() || scope.contains('%')) {
        return None;
    }
    match address.parse::<IpAddr>().ok()? {
        IpAddr::V4(address) => {
            if scope.is_some()
                || address.is_unspecified()
                || address.is_loopback()
                || address.is_link_local()
            {
                return None;
            }
            Some(address.to_string())
        }
        IpAddr::V6(address) => {
            if address.is_unspecified()
                || address.is_loopback()
                || address.is_unicast_link_local()
                || address.to_ipv4_mapped().is_some_and(|mapped| {
                    mapped.is_unspecified() || mapped.is_loopback() || mapped.is_link_local()
                })
            {
                return None;
            }
            let mut text = address.to_string();
            if let Some(scope) = scope {
                text.push('%');
                text.push_str(scope);
            }
            Some(text)
        }
    }
}

pub struct NetworkAddressProvider {
    worker: RefreshWorker<Option<String>>,
}

impl NetworkAddressProvider {
    #[must_use]
    pub fn new(memory: Option<Params>, interval: f64) -> Self {
        Self {
            worker: RefreshWorker::new("cluster-network-address", interval, None, move || {
                let param = memory
                    .as_ref()
                    .and_then(|params| params.get("NetworkAddress").ok().flatten())
                    .and_then(|value| valid_address(&value));
                Ok(param.or_else(socket_address))
            }),
        }
    }

    /// # Errors
    /// Returns an OS worker creation failure.
    pub fn address(&mut self, now: f64) -> io::Result<Option<String>> {
        self.worker.request(now)
    }

    /// # Errors
    /// Returns a worker panic after joining it.
    pub fn close(&mut self) -> io::Result<()> {
        self.worker.close()
    }
}

fn socket_address() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    valid_address(socket.local_addr().ok()?.ip().to_string().as_bytes())
}
