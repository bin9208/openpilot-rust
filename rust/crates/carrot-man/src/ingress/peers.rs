use serde::Serialize;
use std::{collections::BTreeMap, net::SocketAddr};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Fallback {
    Udp,
    Http,
}

#[derive(Default, Debug, Serialize)]
pub struct PeerState {
    revision: u64,
    tcp: BTreeMap<u64, (SocketAddr, u64)>,
    #[serde(skip)]
    fallback: BTreeMap<Fallback, (SocketAddr, u64)>,
}

impl PeerState {
    pub fn selected(&self) -> Option<SocketAddr> {
        let values: Vec<_> = if self.tcp.is_empty() {
            self.fallback.values().collect()
        } else {
            self.tcp.values().collect()
        };
        values.into_iter().max_by_key(|v| v.1).map(|v| v.0)
    }
    pub fn set_tcp(&mut self, token: u64, peer: SocketAddr) -> Option<SocketAddr> {
        self.revision += 1;
        self.tcp.insert(token, (peer, self.revision));
        self.selected()
    }
    pub fn clear_tcp(&mut self, token: u64) -> Option<SocketAddr> {
        self.tcp.remove(&token);
        self.selected()
    }
    pub fn set_fallback(&mut self, transport: Fallback, peer: SocketAddr) -> Option<SocketAddr> {
        self.revision += 1;
        self.fallback.insert(transport, (peer, self.revision));
        self.selected()
    }
    pub fn clear_fallback(&mut self, transport: Fallback) -> Option<SocketAddr> {
        self.fallback.remove(&transport);
        self.selected()
    }
    pub fn clear(&mut self) {
        self.tcp.clear();
        self.fallback.clear();
    }
}
