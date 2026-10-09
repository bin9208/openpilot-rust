use super::{
    capabilities::Capabilities,
    captions::Injector,
    clock::Stamp,
    input::Input,
    keys::Keys,
    profiles::{self, Profile},
    resources::Resources,
    samples::{Samples, Warmup},
    settings::Settings,
    writer::Writer,
};
use crate::Value;
use num_traits::ToPrimitive;
use openpilot_params::Params;
use std::{collections::VecDeque, path::PathBuf};

pub(super) struct State {
    pub state_path: PathBuf,
    pub keys: Keys,
    pub settings: Settings,
    pub capabilities: Capabilities,
    pub input: Input,
    pub writer: Option<Writer>,
    pub connected: bool,
    pub active_quality: u8,
    pub warmup: Warmup,
    pub samples: Samples,
    pub captions: Injector,
    pub resources: Resources,
    pub state: &'static str,
    pub error: String,
    pub events: VecDeque<String>,
    pub bytes_sent: i128,
    pub session_start_bytes: i128,
    pub restart_count: i128,
    pub failures: u32,
    pub next_retry: f64,
    pub last_start: f64,
    pub started: Stamp,
    pub last_frame: Stamp,
    pub frame_id: Option<i128>,
    pub width: u32,
    pub height: u32,
    pub conn_checked: f64,
    pub status_written: f64,
    pub net_checked: f64,
    pub net_ok: bool,
    pub backlog_restarts: u64,
    pub discarded: u64,
}
impl State {
    pub fn new(paths: (PathBuf, PathBuf), params: Option<Params>) -> Self {
        let persisted = super::keys::read(&paths.0);
        let mut state = Self {
            state_path: paths.0,
            keys: Keys::new(paths.1),
            settings: Settings::new(params),
            capabilities: Capabilities::discover(),
            input: Input::default(),
            writer: None,
            connected: false,
            active_quality: 0,
            warmup: Warmup::default(),
            samples: Samples::default(),
            captions: Injector::default(),
            resources: Resources::default(),
            state: "disabled",
            error: String::new(),
            events: VecDeque::new(),
            bytes_sent: 0,
            session_start_bytes: 0,
            restart_count: 0,
            failures: 0,
            next_retry: 0.0,
            last_start: 0.0,
            started: Stamp {
                mono: 0.0,
                wall: 0.0,
            },
            last_frame: Stamp {
                mono: 0.0,
                wall: 0.0,
            },
            frame_id: None,
            width: 526,
            height: 330,
            conn_checked: 0.0,
            status_written: 0.0,
            net_checked: 0.0,
            net_ok: false,
            backlog_restarts: 0,
            discarded: 0,
        };
        let first = if persisted.get("bytes_sent").truth() {
            persisted.get("bytes_sent").int()
        } else {
            Value::integer(0).int()
        };
        if let Ok(bytes) = first {
            state.bytes_sent = bytes.to_i128().unwrap_or(0);
            if let Ok(restarts) = if persisted.get("restart_count").truth() {
                persisted.get("restart_count").int()
            } else {
                Value::integer(0).int()
            } {
                state.restart_count = restarts.to_i128().unwrap_or(0);
            }
        }
        state
    }
    pub fn profile(&mut self, mono: f64) -> Profile {
        profiles::selected(self.settings.integer("CarrotYouTubeQuality", mono))
    }
    pub fn enabled(&mut self, mono: f64) -> bool {
        self.settings.boolean("CarrotYouTubeLive", mono)
    }
    pub fn log(&mut self, message: &str, level: &str) {
        let message = message.trim();
        if message.is_empty() {
            return;
        }
        if self.events.len() == 50 {
            self.events.pop_front();
        }
        self.events.push_back(format!("{level}: {message}"));
    }
    pub fn log_tail(&self) -> Value {
        Value::Array(
            self.events
                .iter()
                .skip(self.events.len().saturating_sub(12))
                .map(|text| Value::text(text))
                .collect(),
        )
    }
    pub fn schedule_backoff(&mut self, reason: &str, mono: f64) {
        self.failures = self.failures.saturating_add(1);
        let delay =
            (3_u64.saturating_mul(2_u64.saturating_pow(self.failures.saturating_sub(1)))).min(60);
        self.next_retry = mono + delay.to_f64().unwrap_or(60.0);
        if self.error.is_empty() && !reason.is_empty() {
            self.error = reason.into();
        }
        self.log(
            &format!(
                "retry in {delay}s: {}",
                if reason.is_empty() {
                    self.error.clone()
                } else {
                    reason.into()
                }
            ),
            "warn",
        );
        self.state = "backoff";
    }
}
