pub mod events;
pub mod packet;
mod route_payload;
mod traffic_payload;
use crate::{
    ingress::peers::PeerState, navigation::NavigationRuntime, route::RouteState, serv::CarrotServ,
};
pub use route_payload::route_points;

pub struct Owner {
    pub navigation: NavigationRuntime,
    pub serv: CarrotServ,
    pub route: RouteState,
    pub peers: PeerState,
    pub events: events::EventState,
    pub legacy_timestamps: Vec<(String, i64)>,
    pub ip_address: String,
    pub nodelink: Option<crate::nodelink::NodeLinks<std::fs::File>>,
    pub nodelink_retry: f64,
    /// 0 idle, 1 fetching, 2 ready, 3 failed (nodelink::ensure on its own thread).
    pub nodelink_fetch: std::sync::Arc<std::sync::atomic::AtomicU8>,
}

impl Owner {
    pub fn new(settings: crate::serv::settings::Settings) -> Self {
        Self {
            navigation: NavigationRuntime::default(),
            serv: CarrotServ::new(settings),
            route: RouteState {
                sequence: -1,
                ..RouteState::default()
            },
            peers: PeerState::default(),
            events: events::EventState::default(),
            legacy_timestamps: Vec::new(),
            ip_address: "0.0.0.0".into(),
            nodelink: None,
            nodelink_retry: 0.,
            nodelink_fetch: std::sync::Arc::default(),
        }
    }
    pub fn stale_rgdata(&mut self, timestamp: i64, session: &str) -> bool {
        if timestamp <= 0 {
            return false;
        }
        if let Some((_, previous)) = self
            .legacy_timestamps
            .iter_mut()
            .find(|(key, _)| key == session)
        {
            if timestamp <= *previous {
                return true;
            }
            *previous = timestamp;
        } else {
            if self.legacy_timestamps.len() >= 16 {
                self.legacy_timestamps.remove(0);
            }
            self.legacy_timestamps.push((session.into(), timestamp));
        }
        false
    }
}
