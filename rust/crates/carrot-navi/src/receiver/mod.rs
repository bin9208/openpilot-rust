mod inputs;
mod snapshots;
pub use snapshots::Dashboard;

#[derive(Clone, Copy)]
pub struct Stream<'a> {
    pub session: &'a str,
    pub kind: &'a str,
    pub name: &'a str,
    pub peer: &'a str,
}

use crate::{
    json::Value,
    manifest::{MapConfig, CATALOG},
    record::{Clock, Record},
    Error,
};
use num_bigint::BigInt;
use num_traits::Zero;
use std::{collections::VecDeque, sync::Arc};

#[derive(Debug)]
pub struct Receiver {
    pub port: Value,
    pub config: MapConfig,
    pub session_id: Option<String>,
    app_version: String,
    manifest: Option<Value>,
    records: Vec<(String, Arc<Record>)>,
    binary_configs: Vec<(String, Arc<Record>)>,
    binary_keyframes: Vec<(String, Arc<Record>)>,
    received_count: BigInt,
    session_received_count: BigInt,
    last_received_at_ms: i128,
    last_peer: String,
    last_error: Option<Value>,
    control_connections: BigInt,
    control_events: VecDeque<Value>,
    state_generation: BigInt,
    media_generation: BigInt,
    media_updates: VecDeque<Arc<Record>>,
    state_changed: bool,
    cereal_publish_count: BigInt,
    last_cereal_publish_mono_ns: u128,
    cereal_error: Option<Value>,
    navigation_log_values: Vec<(String, Vec<u32>)>,
}

impl Receiver {
    pub fn new(port: Value, config: MapConfig) -> Self {
        Self {
            port,
            config,
            session_id: None,
            app_version: String::new(),
            manifest: None,
            records: Vec::new(),
            binary_configs: Vec::new(),
            binary_keyframes: Vec::new(),
            received_count: BigInt::zero(),
            session_received_count: BigInt::zero(),
            last_received_at_ms: 0,
            last_peer: "-".into(),
            last_error: None,
            control_connections: BigInt::zero(),
            control_events: VecDeque::new(),
            state_generation: BigInt::zero(),
            media_generation: BigInt::zero(),
            media_updates: VecDeque::new(),
            state_changed: false,
            cereal_publish_count: BigInt::zero(),
            last_cereal_publish_mono_ns: 0,
            cereal_error: None,
            navigation_log_values: Vec::new(),
        }
    }
    pub fn negotiate(
        &mut self,
        requirements: &Value,
        app_version: &str,
        entropy: impl FnOnce() -> Result<String, Error>,
    ) -> Result<Value, Error> {
        if !requirements.get("type").text_eq("requirements_query")
            || !requirements.get("protocol_version").number_eq(2)
        {
            return Err(Error::value("invalid v2 requirements query"));
        }
        let revision = requirements.get("catalog_revision");
        if matches!(revision, Value::Bool(_)) || !revision.number_eq(1) {
            return Err(Error::value("unsupported v2 catalog revision"));
        }
        let Value::Array(streams) = requirements.get("streams") else {
            return Err(Error::value(
                "app v2 catalog does not contain exactly 28 items",
            ));
        };
        if streams.len() != CATALOG.len() {
            return Err(Error::value(
                "app v2 catalog does not contain exactly 28 items",
            ));
        }
        let mut offered = Vec::with_capacity(CATALOG.len());
        for stream in streams {
            if !matches!(stream, Value::Object(_)) || !stream.get("schema_version").number_eq(1) {
                return Err(Error::value("invalid v2 catalog entry"));
            }
            let index = CATALOG.iter().position(|&(kind, name)| {
                stream.get("kind").text_eq(kind) && stream.get("name").text_eq(name)
            });
            offered.push(index);
        }
        if offered.iter().any(Option::is_none)
            || (0..CATALOG.len())
                .any(|index| offered.iter().filter(|&&item| item == Some(index)).count() != 1)
        {
            return Err(Error::value(
                "app v2 catalog does not match receiver catalog",
            ));
        }
        let session = entropy()?;
        let manifest = self.config.manifest(&session, Value::integer(1));
        self.session_id = Some(session);
        self.app_version = app_version.to_owned();
        self.manifest = Some(manifest.clone());
        self.records.clear();
        self.binary_configs.clear();
        self.binary_keyframes.clear();
        self.media_updates.clear();
        self.session_received_count = BigInt::zero();
        self.control_events.clear();
        self.navigation_log_values.clear();
        self.last_error = None;
        self.mark_state_changed();
        Ok(manifest)
    }
    pub fn set_map_config(&mut self, config: MapConfig) -> bool {
        if config == self.config {
            return false;
        }
        self.config = config;
        self.mark_state_changed();
        true
    }
    pub fn control_connected(&mut self) {
        self.control_connections += 1;
        self.mark_state_changed();
    }
    pub fn control_disconnected(&mut self) {
        self.control_connections =
            (&self.control_connections - BigInt::from(1)).max(BigInt::zero());
        self.mark_state_changed();
    }
    pub fn record_control(&mut self, payload: &Value, peer: &str) -> Result<(), Error> {
        if !payload.get("protocol_version").number_eq(2) {
            return Err(Error::value("unsupported v2 control protocol"));
        }
        self.control_events.push_back(payload.clone());
        if self.control_events.len() > 256 {
            self.control_events.pop_front();
        }
        self.last_peer = peer.to_owned();
        if payload.get("type").text_eq("protocol_error") {
            let default = Value::text("error");
            self.last_error = Some(
                if payload.has("message") {
                    payload.get("message")
                } else if payload.has("code") {
                    payload.get("code")
                } else {
                    &default
                }
                .py_string()?,
            );
        }
        Ok(())
    }
    pub fn fail(&mut self, message: &Value, peer: &str) -> Result<(), Error> {
        self.last_error = Some(message.py_string()?);
        if peer != "-" {
            self.last_peer = peer.to_owned();
        }
        Ok(())
    }
    pub fn stream_config(
        &self,
        session: &str,
        kind: &str,
        name: &str,
        identity: &Value,
    ) -> Result<Value, Error> {
        if Some(session) != self.session_id.as_deref() {
            return Err(Error::value("stale v2 session"));
        }
        let index = CATALOG
            .iter()
            .position(|&entry| entry == (kind, name))
            .filter(|_| !(kind == "image" && name == "lane_top"))
            .ok_or_else(|| Error::value("v2 stream is not enabled"))?;
        if identity.truth() {
            let revision = if identity.has("manifest_revision") {
                identity.get("manifest_revision").int()?
            } else {
                BigInt::from(-1)
            };
            if revision != BigInt::from(1) {
                return Err(Error::value("stale v2 manifest revision"));
            }
            let handle = if identity.has("stream_handle") {
                identity.get("stream_handle").int()?
            } else {
                BigInt::from(-1)
            };
            if handle != BigInt::from(index + 1) {
                return Err(Error::value("v2 stream handle mismatch"));
            }
            if identity.has("schema_version")
                && !identity.get("schema_version").int()?.eq(&BigInt::from(1))
            {
                return Err(Error::value("v2 item schema mismatch"));
            }
        }
        let streams = self
            .manifest
            .as_ref()
            .map(|manifest| manifest.get("streams"));
        match streams {
            Some(Value::Array(streams)) => streams
                .get(index)
                .cloned()
                .ok_or_else(|| Error::value("missing enabled manifest stream")),
            _ => Err(Error::value("missing enabled manifest stream")),
        }
    }
    pub fn record_cereal_publish(
        &mut self,
        error: Option<&Value>,
        clock: &mut impl Clock,
    ) -> Result<(), Error> {
        match error {
            Some(error) => {
                let Value::Text(points) = error.py_string()? else {
                    return Err(Error::value("invalid Python string conversion"));
                };
                self.cereal_error = Some(Value::Text(points.into_iter().take(256).collect()));
            }
            None => {
                self.cereal_publish_count += 1;
                self.last_cereal_publish_mono_ns = clock.mono_ns();
                self.cereal_error = None;
            }
        }
        Ok(())
    }
    pub fn take_state_changed(&mut self) -> bool {
        std::mem::take(&mut self.state_changed)
    }
    pub fn drain_media_updates(&mut self) -> Vec<Arc<Record>> {
        self.media_updates.drain(..).collect()
    }
    pub fn media_bootstrap(&self) -> Vec<Arc<Record>> {
        let mut result = Vec::new();
        for records in [&self.binary_configs, &self.binary_keyframes] {
            let mut ordered: Vec<_> = records.iter().collect();
            ordered.sort_by(|a, b| a.0.cmp(&b.0));
            result.extend(ordered.into_iter().map(|(_, record)| Arc::clone(record)));
        }
        let mut images: Vec<_> = self
            .records
            .iter()
            .filter(|(_, record)| record.present && record.kind == "image")
            .collect();
        images.sort_by(|a, b| a.0.cmp(&b.0));
        result.extend(images.into_iter().map(|(_, record)| Arc::clone(record)));
        result
    }
    fn mark_state_changed(&mut self) {
        self.state_generation += 1;
        self.state_changed = true;
    }
    fn mark_received(&mut self, peer: &str, clock: &mut impl Clock) {
        self.received_count += 1;
        self.session_received_count += 1;
        self.last_received_at_ms = clock.wall_ms();
        self.last_peer = peer.to_owned();
        self.last_error = None;
    }
    fn validate_sequence(&self, key: &str, sequence: &BigInt) -> Result<(), Error> {
        if sequence < &BigInt::zero() {
            return Err(Error::value("invalid v2 sequence"));
        }
        if self
            .records
            .iter()
            .find(|(name, _)| name == key)
            .is_some_and(|(_, record)| sequence <= &record.sequence)
        {
            return Err(Error::value("stale v2 sequence"));
        }
        Ok(())
    }
}

fn insert(records: &mut Vec<(String, Arc<Record>)>, key: &str, record: Arc<Record>) {
    if let Some((_, existing)) = records.iter_mut().find(|(name, _)| name == key) {
        *existing = record;
    } else {
        records.push((key.to_owned(), record));
    }
}
