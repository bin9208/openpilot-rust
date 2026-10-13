use super::{pipeline::Packet, wire};
use crate::web_sound::socket;
use crate::Error;
use futures_util::SinkExt;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    time::Duration,
};
use tokio::sync::{watch, Notify};
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone)]
pub(super) struct Close {
    pub code: u16,
    pub reason: &'static str,
}
pub(super) struct Pending {
    media: bool,
    frames: RefCell<VecDeque<Message>>,
    ready: Notify,
    pub closed: watch::Sender<Option<Close>>,
}
impl Pending {
    fn new(media: bool) -> Rc<Self> {
        let (closed, _) = watch::channel(None);
        Rc::new(Self {
            media,
            frames: RefCell::new(VecDeque::new()),
            ready: Notify::new(),
            closed,
        })
    }
    fn push(&self, message: Message) {
        let mut frames = self.frames.borrow_mut();
        let limit = if self.media { 12 } else { 2 };
        if frames.len() == limit {
            if self.media {
                self.closed.send_replace(Some(Close {
                    code: 1013,
                    reason: "carrot_navi_client_slow",
                }));
                return;
            }
            frames.pop_front();
        }
        frames.push_back(message);
        self.ready.notify_one();
    }
    pub async fn send(&self, sink: socket::Sink) {
        loop {
            let notified = self.ready.notified();
            let message = self.frames.borrow_mut().pop_front();
            let Some(message) = message else {
                notified.await;
                continue;
            };
            if !matches!(
                tokio::time::timeout(Duration::from_secs(1), async {
                    sink.lock().await.send(message).await
                })
                .await,
                Ok(Ok(()))
            ) {
                break;
            }
        }
    }
}
struct Client {
    identity: String,
    include_map: bool,
    pending: Rc<Pending>,
}
#[derive(Default)]
pub(super) struct Clients {
    states: BTreeMap<u64, Client>,
    media: BTreeMap<u64, Client>,
    owner: String,
}
impl Clients {
    pub fn counts(&self) -> (usize, usize) {
        (self.states.len(), self.media.len())
    }
    pub fn has_clients(&self) -> bool {
        !self.states.is_empty() || !self.media.is_empty()
    }
    pub fn wants_map(&self) -> bool {
        self.media.values().any(|client| client.include_map)
    }
    pub fn has_media(&self) -> bool {
        !self.media.is_empty()
    }
    pub fn claim(&mut self, identity: &str, takeover: bool) -> bool {
        if !self.owner.is_empty() && self.owner != identity {
            if !takeover {
                return false;
            }
            for clients in [&mut self.states, &mut self.media] {
                clients.retain(|_, client| {
                    if client.identity == identity {
                        return true;
                    }
                    client.pending.closed.send_replace(Some(Close {
                        code: 4401,
                        reason: "carrot_navi_replaced",
                    }));
                    false
                });
            }
        }
        self.owner = identity.into();
        true
    }
    pub fn register(
        &mut self,
        id: u64,
        identity: String,
        include_map: Option<bool>,
        initial: Vec<Message>,
    ) -> Rc<Pending> {
        let pending = Pending::new(include_map.is_some());
        for message in initial
            .into_iter()
            .take(if include_map.is_some() { 12 } else { 2 })
        {
            pending.push(message);
        }
        let client = Client {
            identity,
            include_map: include_map.unwrap_or(false),
            pending: Rc::clone(&pending),
        };
        if include_map.is_some() {
            self.media.insert(id, client);
        } else {
            self.states.insert(id, client);
        }
        pending
    }
    pub fn unregister(&mut self, id: u64) -> bool {
        let client = self.states.remove(&id).or_else(|| self.media.remove(&id));
        let lost_map = client.is_some_and(|client| client.include_map) && !self.wants_map();
        if !self
            .states
            .values()
            .chain(self.media.values())
            .any(|client| client.identity == self.owner)
        {
            self.owner.clear();
        }
        lost_map
    }
    pub fn state(&self, text: &str) {
        for client in self.states.values() {
            client.pending.push(Message::text(text));
        }
    }
    pub fn media(&self, packets: Vec<Packet>) {
        for packet in packets {
            for client in self.media.values() {
                if !packet.is_map || client.include_map {
                    client
                        .pending
                        .push(Message::Binary(packet.wire.clone().into()));
                }
            }
        }
    }
    pub fn close(&mut self) {
        for client in self.states.values().chain(self.media.values()) {
            client.pending.closed.send_replace(Some(Close {
                code: 1000,
                reason: "",
            }));
        }
    }
    pub fn accepted() -> Result<String, Error> {
        wire::session("accepted", "carrot_navi_accepted")
    }
}
