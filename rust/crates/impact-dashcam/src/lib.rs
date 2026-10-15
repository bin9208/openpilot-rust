//! Source-pinned impact notice handshake; no sensor, UI, or reboot syscall ownership.
use serde::Serialize;
use serde_json::{json, Value};
use std::io;
pub const NOTICE_KEY: &str = "ImpactDashcamNotice";
pub const FEEDBACK_KEY: &str = "ImpactDashcamFeedback";
pub const REBOOT_KEY: &str = "ImpactDashcamReboot";
pub const COUNTDOWN_SECONDS: f64 = 10.;
pub const UI_TIMEOUT: f64 = 0.5;
pub trait Store {
    fn get(&mut self, key: &str) -> io::Result<Option<Vec<u8>>>;
    fn put(&mut self, key: &str, value: &[u8]) -> io::Result<()>;
    fn remove(&mut self, key: &str) -> io::Result<()>;
}
impl Store for &openpilot_params::Params {
    fn get(&mut self, key: &str) -> io::Result<Option<Vec<u8>>> {
        openpilot_params::Params::get(self, key).map_err(io::Error::other)
    }
    fn put(&mut self, key: &str, value: &[u8]) -> io::Result<()> {
        openpilot_params::Params::put(self, key, value).map_err(io::Error::other)
    }
    fn remove(&mut self, key: &str) -> io::Result<()> {
        match openpilot_params::Params::remove(self, key) {
            Ok(()) => Ok(()),
            Err(openpilot_params::Error::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                Ok(())
            }
            Err(error) => Err(io::Error::other(error)),
        }
    }
}
pub fn read_object(store: &mut impl Store, key: &str) -> io::Result<Value> {
    Ok(store
        .get(key)?
        .and_then(|v| serde_json::from_slice::<Value>(&v).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({})))
}
#[derive(Serialize)]
pub struct ImpactDashcam {
    pub pending: bool,
    pub committed: bool,
    pub armed: bool,
    pub quiet_since: Option<f64>,
    pub token: String,
    pub created: f64,
    pub shown_since: Option<f64>,
    pub last_visible: Option<f64>,
}
impl ImpactDashcam {
    pub fn new(params: &mut impl Store, memory: &mut impl Store) -> io::Result<Self> {
        let committed = params.get(REBOOT_KEY)?.as_deref() == Some(b"1");
        memory.remove(NOTICE_KEY)?;
        memory.remove(FEEDBACK_KEY)?;
        Ok(Self {
            pending: false,
            committed,
            armed: true,
            quiet_since: None,
            token: String::new(),
            created: 0.,
            shown_since: None,
            last_visible: None,
        })
    }
    pub fn cancel(&mut self, memory: &mut impl Store) -> io::Result<()> {
        self.pending = false;
        memory.remove(NOTICE_KEY)?;
        memory.remove(FEEDBACK_KEY)
    }
    #[allow(clippy::too_many_arguments)] // Keep the pinned handshake inputs explicit.
    pub fn update(
        &mut self,
        now: f64,
        allowed: bool,
        trigger: bool,
        quiet: Option<bool>,
        details: Value,
        params: &mut impl Store,
        memory: &mut impl Store,
    ) -> io::Result<()> {
        if self.committed {
            return Ok(());
        }
        if !allowed {
            if self.pending {
                self.cancel(memory)?;
            }
            self.quiet_since = None;
            return Ok(());
        }
        if !self.pending {
            if quiet == Some(true) {
                let since = *self.quiet_since.get_or_insert(now);
                if now - since >= 1. {
                    self.armed = true;
                }
            } else if quiet.is_some() {
                self.quiet_since = None;
            }
            if !(trigger && self.armed) {
                return Ok(());
            }
            self.armed = false;
            self.pending = true;
            self.created = now;
            self.token = format!("{:.0}", (now * 1e9).trunc());
            self.shown_since = None;
            self.last_visible = None;
            memory.remove(FEEDBACK_KEY)?;
            memory.put(
                NOTICE_KEY,
                &serde_json::to_vec(&json!({"token":self.token,"created":now,"details":details}))?,
            )?;
        }
        let feedback = read_object(memory, FEEDBACK_KEY)?;
        if feedback["token"].as_str() == Some(&self.token) {
            if feedback["cancel"].as_bool() == Some(true) {
                self.cancel(memory)?;
                return Ok(());
            }
            if let Some(visible) = feedback["visible"].as_f64().filter(|v| {
                v.is_finite() && self.created <= *v && *v <= now && now - *v <= UI_TIMEOUT
            }) {
                if self
                    .last_visible
                    .is_some_and(|last| visible - last > UI_TIMEOUT)
                {
                    self.cancel(memory)?;
                    return Ok(());
                }
                let shown = *self.shown_since.get_or_insert(visible);
                self.last_visible = Some(visible);
                if visible - shown >= COUNTDOWN_SECONDS {
                    params.put("OpenpilotEnabledToggle", b"0")?;
                    if params.get("OpenpilotEnabledToggle")?.as_deref() != Some(b"0") {
                        return Err(io::Error::other("Failed to save OpenpilotEnabledToggle=0"));
                    }
                    params.put(REBOOT_KEY, b"1")?;
                    if params.get(REBOOT_KEY)?.as_deref() != Some(b"1") {
                        return Err(io::Error::other("Failed to save impact reboot interlock"));
                    }
                    self.committed = true;
                    self.pending = false;
                    params.put("DoReboot", b"1")?;
                    return Ok(());
                }
            }
        }
        if self
            .last_visible
            .is_some_and(|last| now - last > UI_TIMEOUT)
            || (self.shown_since.is_none() && now - self.created > 2.)
        {
            self.cancel(memory)?;
        }
        Ok(())
    }
}
