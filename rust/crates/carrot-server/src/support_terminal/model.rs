use super::{
    policy::{self, Permission, StartOptions},
    random,
    tunnel::Tunnel,
};
use crate::{web_sound::socket, Error, Value};
use num_traits::ToPrimitive;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::{watch, Notify};

pub(super) type Session = Rc<RefCell<SessionData>>;
#[derive(Clone)]
pub(super) struct Client {
    pub sink: socket::Sink,
    pub closed: Arc<AtomicBool>,
    pub finished: Arc<Notify>,
    pub terminal_id: Option<u64>,
}
pub(super) struct Pending {
    pub id: String,
    pub line: Value,
    pub created: f64,
    pub control: String,
}
impl Pending {
    pub fn snapshot(&self) -> Value {
        Value::object([
            ("id", Value::text(&self.id)),
            ("line", self.line.clone()),
            ("status", Value::text("pending")),
            ("created_at", Value::Float(self.created)),
        ])
    }
    pub fn request(&self) -> Value {
        Value::object([
            ("type", Value::text("command_request")),
            ("id", Value::text(&self.id)),
            ("line", self.line.clone()),
            ("created_at", Value::Float(self.created)),
            ("control_action", Value::text(&self.control)),
        ])
    }
}
pub(super) struct SessionData {
    pub id: String,
    pub pin: String,
    pub options: StartOptions,
    pub created: f64,
    pub expires: f64,
    pub state: &'static str,
    pub url: String,
    pub local_url: String,
    pub origin: String,
    pub discord: Value,
    pub error: String,
    pub detail: String,
    pub failures: usize,
    pub pending: Vec<Pending>,
    pub tunnel: Option<Tunnel>,
    pub local_stop: Option<watch::Sender<bool>>,
    pub expiry_stop: watch::Sender<bool>,
    pub owners: BTreeMap<u64, Client>,
    pub guests: BTreeMap<u64, Client>,
    pub controller: Option<u64>,
}
impl SessionData {
    pub fn new(options: StartOptions) -> Result<Session, Error> {
        let created = policy::now();
        let expires = created
            + options
                .ttl
                .to_f64()
                .ok_or_else(|| Error::Source("TTL range".into()))?;
        let (expiry_stop, _) = watch::channel(false);
        Ok(Rc::new(RefCell::new(Self {
            id: random::token::<16>()?,
            pin: random::pin()?,
            options,
            created,
            expires,
            state: "starting",
            url: String::new(),
            local_url: String::new(),
            origin: String::new(),
            discord: Value::object([]),
            error: String::new(),
            detail: String::new(),
            failures: 0,
            pending: Vec::new(),
            tunnel: None,
            local_stop: None,
            expiry_stop,
            owners: BTreeMap::new(),
            guests: BTreeMap::new(),
            controller: None,
        })))
    }
    pub fn expired(&self) -> bool {
        self.expires > 0. && policy::now() >= self.expires
    }
    pub fn remaining(&self) -> Value {
        if self.expires <= 0. {
            Value::Null
        } else {
            Value::integer((self.expires - policy::now()).max(0.).to_i64().unwrap_or(0))
        }
    }
    pub fn controller_present(&self) -> bool {
        self.controller.is_some_and(|id| {
            self.guests
                .get(&id)
                .is_some_and(|client| !client.closed.load(Ordering::SeqCst))
        })
    }
    pub fn snapshot(&self) -> Value {
        Value::object([
            ("ok", Value::Bool(true)),
            (
                "active",
                Value::Bool(matches!(self.state, "starting" | "sharing")),
            ),
            ("id", Value::text(&self.id)),
            ("state", Value::text(self.state)),
            ("url", Value::text(&self.url)),
            ("local_url", Value::text(&self.local_url)),
            ("expires_at", Value::Float(self.expires)),
            ("expires_in", self.remaining()),
            ("ttl_seconds", Value::integer(self.options.ttl)),
            (
                "permission_mode",
                Value::text(self.options.permission.name()),
            ),
            (
                "command_timeout_seconds",
                Value::integer(self.options.command_timeout),
            ),
            ("guest_count", Value::integer(self.guests.len())),
            ("owner_count", Value::integer(self.owners.len())),
            ("owner_present", Value::Bool(!self.owners.is_empty())),
            ("controller_present", Value::Bool(self.controller_present())),
            ("discord", self.discord.clone()),
            ("error", Value::text(&self.error)),
            ("status_detail", Value::text(&self.detail)),
            (
                "pending_commands",
                Value::Array(self.pending.iter().map(Pending::snapshot).collect()),
            ),
        ])
    }
    pub fn grant(&self, id: u64) -> bool {
        self.options.permission == Permission::ApproveEach || self.controller == Some(id)
    }
}
impl Drop for SessionData {
    fn drop(&mut self) {
        self.expiry_stop.send_replace(true);
        if let Some(stop) = &self.local_stop {
            stop.send_replace(true);
        }
    }
}
pub(super) fn idle() -> Value {
    Value::object([
        ("ok", Value::Bool(true)),
        ("active", Value::Bool(false)),
        ("state", Value::text("idle")),
    ])
}
