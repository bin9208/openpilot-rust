use super::{camera_frame, socket, value};
use futures_util::{future::join_all, SinkExt};
use openpilot_msgq::Subscriber;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    time::{Duration, Instant},
};
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::Message;

pub(super) struct Client {
    pub sink: socket::Sink,
    pub failed: Rc<Notify>,
    failures: u8,
}
pub(super) struct CameraHub {
    pub clients: BTreeMap<u64, Client>,
    sockets: BTreeMap<&'static str, Subscriber>,
    queue: VecDeque<Vec<u8>>,
    pub ready: Rc<Notify>,
    ready_id: Option<i128>,
    codec: String,
    next: Instant,
    running: bool,
    idle: Option<Instant>,
}
impl CameraHub {
    pub fn new() -> Self {
        Self {
            clients: BTreeMap::new(),
            sockets: BTreeMap::new(),
            queue: VecDeque::new(),
            ready: Rc::new(Notify::new()),
            ready_id: None,
            codec: String::new(),
            next: Instant::now(),
            running: false,
            idle: None,
        }
    }
    pub fn register(&mut self, id: u64, sink: socket::Sink, failed: Rc<Notify>) {
        self.clients.insert(
            id,
            Client {
                sink,
                failed,
                failures: 0,
            },
        );
        self.running = true;
        self.idle = None;
    }
    pub fn unregister(&mut self, id: u64) {
        self.clients.remove(&id);
    }
    pub fn delay(&self) -> Option<Duration> {
        self.running.then(|| {
            if self.clients.is_empty() {
                Duration::from_millis(30)
            } else {
                self.next
                    .saturating_duration_since(Instant::now())
                    .max(Duration::from_millis(1))
            }
        })
    }
    fn receive(&mut self, name: &'static str) -> Option<Vec<u8>> {
        if !self.sockets.contains_key(name) {
            let capacity = openpilot_messaging::services::lookup(name)
                .map_or(1024 * 1024, |service| service.queue_size);
            let Ok(socket) = Subscriber::for_runtime(name, true, capacity) else {
                return None;
            };
            self.sockets.insert(name, socket);
        }
        match self
            .sockets
            .get_mut(name)
            .map(|socket| socket.receive(Duration::ZERO))
        {
            Some(Ok(bytes)) => bytes,
            Some(Err(_)) | None => None,
        }
    }
    pub fn poll(&mut self) {
        if !self.running {
            return;
        }
        let now = Instant::now();
        if self.clients.is_empty() {
            let idle = self.idle.get_or_insert(now);
            while self.queue.len() > 1 {
                self.queue.pop_front();
            }
            if idle.elapsed() >= Duration::from_secs(5) {
                self.running = false;
                self.sockets.clear();
                self.ready_id = None;
            }
            return;
        }
        self.idle = None;
        if now < self.next {
            return;
        }
        if let Some(bytes) = self.receive("roadCameraState") {
            if let Ok(message) = value::message(&bytes) {
                if let Ok(frame) = value::service(&message, "roadCameraState") {
                    let id = value::integer(value::field(frame, "frameId"));
                    if id > 0 {
                        self.ready_id = Some(id);
                    }
                }
            }
        }
        let Some(ready_id) = self.ready_id else {
            self.next = now + Duration::from_millis(30);
            return;
        };
        self.next = now + Duration::from_millis(2);
        for name in ["livestreamRoadEncodeData", "roadEncodeData"] {
            let Some(bytes) = self.receive(name) else {
                continue;
            };
            let packet = value::message(&bytes).and_then(|message| {
                value::service(&message, name)
                    .and_then(|frame| camera_frame::pack(frame, &mut self.codec, ready_id))
            });
            if let Ok(packet) = packet {
                if self.queue.len() == 8 {
                    self.queue.pop_front();
                }
                self.queue.push_back(packet);
                self.ready.notify_one();
            }
            break;
        }
    }
    pub async fn send(hub: Rc<RefCell<Self>>) {
        let ready = Rc::clone(&hub.borrow().ready);
        loop {
            let notified = ready.notified();
            let selected = {
                let mut hub = hub.borrow_mut();
                if hub.clients.is_empty() {
                    None
                } else {
                    hub.queue.pop_front().map(|mut packet| {
                        while hub.queue.len() > 1 {
                            if let Some(newer) = hub.queue.pop_front() {
                                packet = newer;
                            }
                        }
                        (
                            packet,
                            hub.clients
                                .iter()
                                .map(|(id, client)| (*id, std::sync::Arc::clone(&client.sink)))
                                .collect::<Vec<_>>(),
                        )
                    })
                }
            };
            let Some((packet, clients)) = selected else {
                notified.await;
                continue;
            };
            let outcomes = join_all(clients.into_iter().map(|(id, sink)| {
                let packet = packet.clone();
                async move {
                    (
                        id,
                        matches!(
                            tokio::time::timeout(Duration::from_millis(350), async {
                                sink.lock().await.send(Message::Binary(packet.into())).await
                            })
                            .await,
                            Ok(Ok(()))
                        ),
                    )
                }
            }))
            .await;
            let mut hub = hub.borrow_mut();
            let mut stale = Vec::new();
            for (id, ok) in outcomes {
                if let Some(client) = hub.clients.get_mut(&id) {
                    if ok {
                        client.failures = 0;
                    } else {
                        client.failures = client.failures.saturating_add(1);
                        if client.failures >= 4 {
                            client.failed.notify_one();
                            stale.push(id);
                        }
                    }
                }
            }
            for id in stale {
                hub.clients.remove(&id);
            }
        }
    }
}
