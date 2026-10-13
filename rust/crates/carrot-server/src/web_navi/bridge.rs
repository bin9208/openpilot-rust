use super::{
    clients::{Clients, Pending},
    pipeline::Pipeline,
};
use crate::{Error, Value};
use openpilot_msgq::Subscriber;
use openpilot_params::Params;
use std::{
    collections::BTreeMap,
    rc::Rc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::Message;

pub(super) struct Bridge {
    pub clients: Clients,
    pub pipeline: Pipeline,
    pub params: Option<Params>,
    pub running: bool,
    pub wake: Rc<Notify>,
    pub last_state: Option<Value>,
    pub last_wire: Option<String>,
    pub state_at: Option<Instant>,
    pub state_count: usize,
    pub media_count: usize,
    pub diagnostics: BTreeMap<String, (Instant, Value)>,
    pub hud_clients: usize,
    pub error: String,
    pub(super) state_socket: Option<Subscriber>,
    pub(super) media_socket: Option<Subscriber>,
    idle_since: Option<Instant>,
    allowed: bool,
    next_gate: Instant,
}
impl Bridge {
    pub fn new(params: Option<Params>, wake: Rc<Notify>) -> Self {
        if let Some(params) = &params {
            let _ = params.put("CarrotNaviHudMapProfile", b"0");
        }
        Self {
            clients: Clients::default(),
            pipeline: Pipeline::default(),
            params,
            running: false,
            wake,
            last_state: None,
            last_wire: None,
            state_at: None,
            state_count: 0,
            media_count: 0,
            diagnostics: BTreeMap::new(),
            hud_clients: 0,
            error: String::new(),
            state_socket: None,
            media_socket: None,
            idle_since: None,
            allowed: false,
            next_gate: Instant::now(),
        }
    }
    pub fn allowed(&mut self, force: bool) -> bool {
        if !force && Instant::now() < self.next_gate {
            return self.allowed;
        }
        self.next_gate = Instant::now() + Duration::from_millis(100);
        self.allowed = self.params.as_ref().is_some_and(|params| {
            let bytes = params.get("ClusterHud").ok().flatten().unwrap_or_default();
            match openpilot_beepd::integer(&bytes) {
                Ok(value) => value != 1,
                Err(error) => crate::param_native::fatal("ClusterHud", &error),
            }
        });
        self.allowed
    }
    fn subscribe(name: &str, conflate: bool) -> Result<Subscriber, Error> {
        let capacity = openpilot_messaging::services::lookup(name)
            .map_or(1024 * 1024, |service| service.queue_size);
        Subscriber::for_runtime(name, conflate, capacity)
            .map_err(|error| Error::Source(error.to_string()))
    }
    pub fn sockets(&mut self) -> Result<(), Error> {
        if self.clients.has_clients() && self.state_socket.is_none() {
            self.state_socket = Some(Self::subscribe("carrotNavi", true)?);
        }
        if self.clients.has_media() && self.media_socket.is_none() {
            self.media_socket = Some(Self::subscribe("carrotNaviMedia", false)?);
        }
        Ok(())
    }
    pub fn register(
        &mut self,
        id: u64,
        identity: String,
        mode: Option<bool>,
        takeover: bool,
    ) -> Option<Rc<Pending>> {
        let counts = self.clients.counts();
        if !self.clients.claim(&identity, takeover) {
            return None;
        }
        if self.clients.counts() != counts && !self.clients.wants_map() {
            self.pipeline.reset(false);
        }
        let initial = match mode {
            Some(include_map) => self
                .pipeline
                .bootstrap(include_map)
                .into_iter()
                .map(|wire| Message::Binary(wire.into()))
                .collect(),
            None => self
                .last_wire
                .as_ref()
                .filter(|_| {
                    self.state_at
                        .is_some_and(|at| at.elapsed() <= Duration::from_secs(3))
                })
                .map(|wire| vec![Message::text(wire)])
                .unwrap_or_default(),
        };
        let empty_bootstrap = initial.is_empty();
        let pending = self.clients.register(id, identity, mode, initial);
        self.running = true;
        self.idle_since = None;
        if let Err(error) = self.sockets() {
            self.error = error.to_string().chars().take(256).collect();
        }
        if let Some(include_map) = mode {
            if empty_bootstrap || (include_map && self.pipeline.initialization.is_none()) {
                if let Some(params) = &self.params {
                    let token = format!(
                        "{:x}",
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos()
                    );
                    let _ = params.put("CarrotNaviWebBootstrapRequest", token.as_bytes());
                }
            }
        }
        self.wake.notify_one();
        Some(pending)
    }
    pub fn unregister(&mut self, id: u64) {
        if self.clients.unregister(id) {
            self.pipeline.reset(false);
        }
        self.wake.notify_one();
    }
    pub fn profile(&mut self, active: bool) {
        if active {
            self.hud_clients += 1;
        } else {
            self.hud_clients = self.hud_clients.saturating_sub(1);
        }
        if let Some(params) = &self.params {
            let _ = params.put(
                "CarrotNaviHudMapProfile",
                if self.hud_clients > 0 { b"1" } else { b"0" },
            );
        }
    }
    fn clear_cache(&mut self) {
        self.last_state = None;
        self.last_wire = None;
        self.state_at = None;
        self.pipeline.clear();
    }
    pub fn stop(&mut self) {
        self.clients.close();
        self.hud_clients = 0;
        self.profile(false);
        self.clear_cache();
        self.state_socket = None;
        self.media_socket = None;
        self.running = false;
    }
    pub fn poll(&mut self) -> Duration {
        if !self.allowed(false) {
            self.error = "Cluster HUD active".into();
            self.clients.close();
            self.clear_cache();
            self.state_socket = None;
            self.media_socket = None;
            self.running = false;
            return Duration::from_millis(4);
        }
        if !self.clients.has_clients() {
            if self
                .idle_since
                .is_some_and(|at| at.elapsed() >= Duration::from_secs(2))
            {
                self.state_socket = None;
                self.media_socket = None;
                self.running = false;
            } else if self.idle_since.is_none() {
                self.idle_since = Some(Instant::now());
            }
            return Duration::from_millis(30);
        }
        self.idle_since = None;
        if !self.clients.has_media() {
            self.media_socket = None;
        }
        let result = self.sockets().and_then(|_| self.receive());
        match result {
            Ok(()) => Duration::from_millis(4),
            Err(error) => {
                self.error = error.to_string().chars().take(256).collect();
                Duration::from_millis(100)
            }
        }
    }
}
