use super::{compact, socket, Mode};
use futures_util::SinkExt;
use openpilot_msgq::Subscriber;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
    time::{Duration, Instant},
};
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::Message;

pub(super) struct Pending {
    pub mode: Mode,
    pub services: Vec<String>,
    frames: RefCell<VecDeque<(String, Vec<u8>)>>,
    ready: Notify,
}
impl Pending {
    pub fn new(mode: Mode, services: Vec<String>) -> Self {
        Self {
            mode,
            services,
            frames: RefCell::new(VecDeque::new()),
            ready: Notify::new(),
        }
    }
    fn push(&self, service: &str, bytes: Vec<u8>) {
        let mut frames = self.frames.borrow_mut();
        frames.retain(|(name, _)| name != service);
        frames.push_back((service.into(), bytes));
        self.ready.notify_one();
    }
    pub async fn send(&self, sink: socket::Sink) -> bool {
        loop {
            let notified = self.ready.notified();
            if self.frames.borrow().is_empty() {
                notified.await;
            }
            if self.mode == Mode::Compact {
                tokio::time::sleep(Duration::from_millis(6)).await;
            }
            let bytes = match self.mode {
                Mode::Compact => Some(compact::batch(
                    self.frames.borrow_mut().drain(..).map(|(_, bytes)| bytes),
                )),
                Mode::Single | Mode::Multiplex => {
                    self.frames.borrow_mut().pop_front().map(|(_, bytes)| bytes)
                }
                Mode::Camera => None,
            };
            let Some(bytes) = bytes else {
                continue;
            };
            if !matches!(
                tokio::time::timeout(Duration::from_millis(750), async {
                    sink.lock().await.send(Message::Binary(bytes.into())).await
                })
                .await,
                Ok(Ok(()))
            ) {
                return false;
            }
        }
    }
}
pub(super) struct RawHub {
    pub clients: BTreeMap<u64, Rc<Pending>>,
    sockets: BTreeMap<String, Subscriber>,
    next: BTreeMap<String, Instant>,
    sequence: BTreeMap<String, u16>,
    running: bool,
    idle: Option<Instant>,
}
impl RawHub {
    pub fn new() -> Self {
        Self {
            clients: BTreeMap::new(),
            sockets: BTreeMap::new(),
            next: BTreeMap::new(),
            sequence: BTreeMap::new(),
            running: false,
            idle: None,
        }
    }
    pub fn register(&mut self, id: u64, pending: Rc<Pending>) {
        self.clients.insert(id, pending);
        self.running = true;
        self.idle = None;
    }
    pub fn unregister(&mut self, id: u64) {
        self.clients.remove(&id);
    }
    pub fn delay(&self) -> Option<Duration> {
        if !self.running {
            return None;
        }
        let now = Instant::now();
        Some(if self.clients.is_empty() {
            Duration::from_millis(30)
        } else {
            self.next
                .values()
                .map(|time| time.saturating_duration_since(now))
                .min()
                .unwrap_or(Duration::from_millis(30))
                .clamp(Duration::from_millis(2), Duration::from_millis(30))
        })
    }
    pub fn poll(&mut self) {
        if !self.running {
            return;
        }
        let now = Instant::now();
        let active: BTreeSet<String> = self
            .clients
            .values()
            .flat_map(|client| client.services.iter().cloned())
            .collect();
        if active.is_empty() {
            let idle = self.idle.get_or_insert(now);
            if idle.elapsed() >= Duration::from_secs(5) {
                self.sockets.clear();
                self.next.clear();
                self.running = false;
            }
            return;
        }
        self.idle = None;
        self.sockets.retain(|name, _| active.contains(name));
        self.next.retain(|name, _| active.contains(name));
        for name in active {
            if self.next.get(&name).is_some_and(|due| now < *due) {
                continue;
            }
            if !self.sockets.contains_key(&name) {
                let capacity = openpilot_messaging::services::lookup(&name)
                    .map_or(1024 * 1024, |service| service.queue_size);
                match Subscriber::for_runtime(&name, true, capacity) {
                    Ok(socket) => {
                        self.sockets.insert(name.clone(), socket);
                    }
                    Err(_) => {
                        self.next.insert(name, now + Duration::from_millis(30));
                        continue;
                    }
                }
            }
            let received = self
                .sockets
                .get_mut(&name)
                .map(|socket| socket.receive(Duration::ZERO));
            let payload = match received {
                Some(Ok(Some(payload))) => payload,
                Some(Ok(None)) | Some(Err(_)) | None => {
                    let delay = Duration::from_millis(compact::interval_ms(&name).min(10));
                    self.next.insert(name, now + delay);
                    continue;
                }
            };
            self.next.insert(
                name.clone(),
                now + Duration::from_millis(compact::interval_ms(&name).max(2)),
            );
            let mut compact_frame = None;
            if self
                .clients
                .values()
                .any(|client| client.mode == Mode::Compact && client.services.contains(&name))
            {
                let sequence = self
                    .sequence
                    .get(&name)
                    .copied()
                    .unwrap_or(0)
                    .wrapping_add(1);
                if let Ok(frame) = compact::encode(&name, &payload, sequence) {
                    self.sequence.insert(name.clone(), sequence);
                    compact_frame = Some(frame);
                }
            }
            for client in self
                .clients
                .values()
                .filter(|client| client.services.contains(&name))
            {
                let frame = match client.mode {
                    Mode::Single => Some(payload.clone()),
                    Mode::Multiplex => {
                        let mut frame = Vec::with_capacity(1 + name.len() + payload.len());
                        frame.push(u8::try_from(name.len()).unwrap_or(0));
                        frame.extend(name.as_bytes());
                        frame.extend(&payload);
                        Some(frame)
                    }
                    Mode::Compact => compact_frame.clone(),
                    Mode::Camera => None,
                };
                if let Some(frame) = frame {
                    client.push(&name, frame);
                }
            }
        }
    }
}
