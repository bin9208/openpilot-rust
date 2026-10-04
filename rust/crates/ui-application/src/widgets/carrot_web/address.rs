use crate::params::Read;
use std::net::{IpAddr, Ipv6Addr};
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Address {
    pub address: String,
    pub url: Option<String>,
}
fn compressed(address: Ipv6Addr) -> String {
    let parts = address.segments();
    let mut best = (0usize, 0usize);
    let mut i = 0;
    while i < 8 {
        if parts[i] != 0 {
            i += 1;
            continue;
        }
        let start = i;
        while i < 8 && parts[i] == 0 {
            i += 1;
        }
        if i - start > best.1 {
            best = (start, i - start);
        }
    }
    if best.1 < 2 {
        return parts.map(|v| format!("{v:x}")).join(":");
    }
    let left = parts[..best.0]
        .iter()
        .map(|v| format!("{v:x}"))
        .collect::<Vec<_>>()
        .join(":");
    let right = parts[best.0 + best.1..]
        .iter()
        .map(|v| format!("{v:x}"))
        .collect::<Vec<_>>()
        .join(":");
    format!("{left}::{right}")
}
pub fn url(address: &str) -> Option<String> {
    let text = openpilot_ui_framework::text::trim(address);
    let (host, scope) = match text.split_once('%') {
        Some((host, scope)) if !scope.is_empty() && !scope.contains('%') => (host, Some(scope)),
        Some(_) => return None,
        None => (text, None),
    };
    let address = host.parse::<IpAddr>().ok()?;
    if address.is_unspecified() || address.is_loopback() || address.is_multicast() {
        return None;
    }
    let host = match address {
        IpAddr::V4(value) => {
            if scope.is_some() {
                return None;
            }
            value.to_string()
        }
        IpAddr::V6(value) => format!(
            "[{}{}]",
            compressed(value),
            scope.map_or(String::new(), |scope| format!("%{scope}"))
        ),
    };
    Some(format!("http://{host}:7000"))
}
pub struct Watcher {
    pub value: Address,
    pub next_refresh: f64,
    pub interval: f64,
}
impl Default for Watcher {
    fn default() -> Self {
        Self {
            value: Address::default(),
            next_refresh: 0.0,
            interval: 0.1,
        }
    }
}
impl Watcher {
    pub fn refresh(&mut self, params: &impl Read, now: f64, force: bool) -> bool {
        if !force && now < self.next_refresh {
            return false;
        }
        self.next_refresh = now + self.interval;
        let address = params.string("NetworkAddress").unwrap_or_default();
        let address = openpilot_ui_framework::text::trim(&address);
        let url = url(address);
        let next = Address {
            address: if url.is_some() {
                address.into()
            } else {
                String::new()
            },
            url,
        };
        let changed = next != self.value;
        self.value = next;
        changed
    }
}
