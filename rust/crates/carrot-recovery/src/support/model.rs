use crate::{websocket::Sender, Error, Value};
use base64::Engine;
use num_traits::ToPrimitive;
use std::{
    collections::BTreeMap,
    sync::{atomic::AtomicBool, Arc, Condvar, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, PartialEq)]
pub enum Permission {
    ApproveEach,
    AllowAll,
}
impl Permission {
    pub fn name(self) -> &'static str {
        match self {
            Self::ApproveEach => "approve_each",
            Self::AllowAll => "allow_all",
        }
    }
}
#[derive(Clone, Copy)]
pub enum State {
    Starting,
    Sharing,
    Error,
    Stopped,
    Expired,
}
impl State {
    fn name(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Sharing => "sharing",
            Self::Error => "error",
            Self::Stopped => "stopped",
            Self::Expired => "expired",
        }
    }
    pub fn active(self) -> bool {
        matches!(self, Self::Starting | Self::Sharing)
    }
}
#[derive(Clone)]
pub struct Command {
    pub id: String,
    pub line: String,
    pub created: f64,
    pub action: String,
}
pub struct Guest {
    pub sender: Arc<Sender>,
    pub terminal: Option<u64>,
}
pub struct Data {
    pub state: State,
    pub url: String,
    pub local_url: String,
    pub error: String,
    pub detail: String,
    pub discord: Value,
    pub failures: u32,
    pub pending: Vec<Command>,
    pub owners: BTreeMap<u64, Arc<Sender>>,
    pub guests: BTreeMap<u64, Guest>,
    pub controller: Option<u64>,
    pub tunnel: Option<super::transport::Tunnel>,
}
pub struct Session {
    pub id: String,
    pub pin: String,
    pub note: String,
    pub created: f64,
    pub expires: f64,
    pub ttl: i64,
    pub timeout: i64,
    pub permission: Permission,
    pub data: Mutex<Data>,
    pub stopped: Arc<AtomicBool>,
    pub wake: Condvar,
    pub wait: Mutex<()>,
    pub sockets: crate::http::registry::Registry,
}
pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0., |value| value.as_secs_f64())
}
pub fn text(value: &Value) -> Result<String, Error> {
    if value.truth() {
        Ok(value.string()?)
    } else {
        Ok(String::new())
    }
}
pub fn token<const N: usize>() -> Result<String, Error> {
    let mut bytes = [0_u8; N];
    getrandom::fill(&mut bytes).map_err(|error| Error::Source(error.to_string()))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}
fn pin() -> Result<String, Error> {
    loop {
        let mut bytes = [0_u8; 4];
        getrandom::fill(&mut bytes).map_err(|error| Error::Source(error.to_string()))?;
        let value = u32::from_ne_bytes(bytes) & 0x000f_ffff;
        if value < 1_000_000 {
            return Ok(format!("{value:06}"));
        }
    }
}
pub fn failure(message: &str) -> Value {
    Value::object([("ok", Value::Bool(false)), ("error", Value::text(message))])
}
pub fn idle() -> Value {
    Value::object([
        ("ok", Value::Bool(true)),
        ("active", Value::Bool(false)),
        ("state", Value::text("idle")),
    ])
}
pub fn event(kind: &str, value: Value) -> Value {
    let mut fields = vec![("type".chars().map(u32::from).collect(), Value::text(kind))];
    if let Value::Object(mut values) = value {
        fields.append(&mut values);
    }
    Value::Object(fields)
}

impl Session {
    pub fn new(
        payload: &Value,
        default_ttl: i64,
        default_timeout: i64,
    ) -> Result<Arc<Self>, Error> {
        let ttl = payload
            .get("ttl_seconds")
            .int()
            .ok()
            .and_then(|v| v.to_i64())
            .unwrap_or(default_ttl);
        let ttl = if [900, 1800, 3600].contains(&ttl) {
            ttl
        } else if [900, 1800, 3600].contains(&default_ttl) {
            default_ttl
        } else {
            1800
        };
        let timeout = payload
            .get("command_timeout_seconds")
            .int()
            .ok()
            .and_then(|v| v.to_i64())
            .unwrap_or(default_timeout);
        let timeout = if [15, 30, 60, 120].contains(&timeout) {
            timeout
        } else if [15, 30, 60, 120].contains(&default_timeout) {
            default_timeout
        } else {
            30
        };
        let permission = match text(payload.get("permission_mode"))?.trim() {
            "allow_all" => Permission::AllowAll,
            _ => Permission::ApproveEach,
        };
        let created = now();
        Ok(Arc::new(Self {
            id: token::<16>()?,
            pin: pin()?,
            note: text(payload.get("note"))?.chars().take(500).collect(),
            created,
            expires: created + ttl.to_f64().unwrap_or(1800.),
            ttl,
            timeout,
            permission,
            data: Mutex::new(Data {
                state: State::Starting,
                url: String::new(),
                local_url: String::new(),
                error: String::new(),
                detail: String::new(),
                discord: Value::Object(Vec::new()),
                failures: 0,
                pending: Vec::new(),
                owners: BTreeMap::new(),
                guests: BTreeMap::new(),
                controller: None,
                tunnel: None,
            }),
            stopped: Arc::new(AtomicBool::new(false)),
            wake: Condvar::new(),
            wait: Mutex::new(()),
            sockets: Arc::new(Mutex::new(BTreeMap::new())),
        }))
    }
    pub fn data(&self) -> Result<MutexGuard<'_, Data>, Error> {
        self.data
            .lock()
            .map_err(|_| Error::Source("support session poisoned".into()))
    }
    pub fn expired(&self) -> bool {
        self.expires > 0. && now() >= self.expires
    }
    pub fn remaining(&self) -> Value {
        if self.expires <= 0. {
            Value::Null
        } else {
            Value::integer((self.expires - now()).max(0.).to_i64().unwrap_or(0))
        }
    }
    pub fn snapshot(&self) -> Result<Value, Error> {
        let data = self.data()?;
        Ok(Value::object([
            ("ok", Value::Bool(true)),
            ("active", Value::Bool(data.state.active())),
            ("id", Value::text(&self.id)),
            ("state", Value::text(data.state.name())),
            ("url", Value::text(&data.url)),
            ("local_url", Value::text(&data.local_url)),
            ("expires_at", Value::Float(self.expires)),
            ("expires_in", self.remaining()),
            ("ttl_seconds", Value::integer(self.ttl)),
            ("permission_mode", Value::text(self.permission.name())),
            ("command_timeout_seconds", Value::integer(self.timeout)),
            ("guest_count", Value::integer(data.guests.len())),
            ("owner_count", Value::integer(data.owners.len())),
            ("owner_present", Value::Bool(!data.owners.is_empty())),
            (
                "controller_present",
                Value::Bool(
                    data.controller
                        .and_then(|id| data.guests.get(&id))
                        .is_some_and(|guest| !guest.sender.is_closed()),
                ),
            ),
            ("discord", data.discord.clone()),
            ("error", Value::text(&data.error)),
            ("status_detail", Value::text(&data.detail)),
            (
                "pending_commands",
                Value::Array(
                    data.pending
                        .iter()
                        .map(|command| {
                            Value::object([
                                ("id", Value::text(&command.id)),
                                ("line", Value::text(&command.line)),
                                ("status", Value::text("pending")),
                                ("created_at", Value::Float(command.created)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]))
    }
    pub fn owners(&self, payload: &Value) -> Result<(), Error> {
        self.data()?
            .owners
            .retain(|_, sender| sender.send_json(payload));
        Ok(())
    }
    pub fn guests(&self, payload: &Value) -> Result<(), Error> {
        self.data()?
            .guests
            .retain(|_, guest| guest.sender.send_json(payload));
        Ok(())
    }
    pub fn all(&self, payload: &Value) -> Result<(), Error> {
        self.owners(payload)?;
        self.guests(payload)
    }
    pub fn detail(&self, detail: &str) -> Result<(), Error> {
        self.data()?.detail = detail.into();
        self.owners(&event("session_status", self.snapshot()?))
    }
}
