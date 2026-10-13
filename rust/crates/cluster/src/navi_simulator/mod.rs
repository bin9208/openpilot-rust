mod dashboard;
mod discovery;
mod media;
mod overlay;
pub use overlay::merge_overlay_state;
mod source;
use crate::{
    media::{decoder, worker::DecodeWorker},
    models::{ClusterUiState, NaviLiveState, NaviMediaFrame, SimulatorInput},
    simulator::ClusterSimulator,
};
use capnp::{message::ReaderOptions, serialize};
use num_traits::ToPrimitive;
use openpilot_carrot_navi::{json::Value, native::wire, receiver::Dashboard};
use openpilot_cereal::log_capnp::event;
pub use source::{Config, Source};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Receiver(#[from] openpilot_carrot_navi::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Clock(#[from] crate::live::Error),
    #[error("navigation value exceeds its runtime representation")]
    Range,
}

pub struct Projection {
    endpoint: String,
    simulator: ClusterSimulator,
    last_update: f64,
    state_generation: Value,
    media_generation: Value,
    session: String,
    navi: Option<NaviLiveState>,
    media: BTreeMap<String, Arc<NaviMediaFrame>>,
    epoch: u64,
    requested: BTreeMap<String, u64>,
    worker: DecodeWorker,
    map_age: Option<i64>,
    stalled: bool,
    last_metadata: Option<Value>,
    last_records: BTreeMap<String, Value>,
}

impl Projection {
    #[must_use]
    pub fn new(endpoint: String, now: f64) -> Self {
        Self {
            endpoint,
            simulator: ClusterSimulator::default(),
            last_update: now,
            state_generation: Value::integer(-1),
            media_generation: Value::integer(-1),
            session: String::new(),
            navi: None,
            media: BTreeMap::new(),
            epoch: 0,
            requested: BTreeMap::new(),
            worker: DecodeWorker::new(decoder::factory()),
            map_age: None,
            stalled: false,
            last_metadata: None,
            last_records: BTreeMap::new(),
        }
    }

    /// # Errors
    /// Returns receiver projection, Cereal conversion or timestamp range errors.
    pub fn update_at(
        &mut self,
        snapshot: &Dashboard,
        cereal: &Value,
        now: f64,
        wall_ms: i128,
        clock: &str,
    ) -> Result<ClusterUiState, Error> {
        let dt = (now - self.last_update).clamp(0.001, 0.25);
        self.last_update = now;
        let metadata = &snapshot.metadata;
        self.last_metadata = Some(metadata.clone());
        self.last_records = snapshot
            .records
            .iter()
            .map(|(key, record)| (key.clone(), record.summary()))
            .collect();
        let session = metadata.get("session_id").string()?;
        let connected = metadata.get("connected").truth();
        if session != self.session {
            self.session = session;
            self.media_generation = Value::integer(-1);
            self.clear_media()?;
        } else if !connected && (!self.media.is_empty() || !self.requested.is_empty()) {
            self.clear_media()?;
        }
        if metadata.get("state_generation") != &self.state_generation {
            self.state_generation = metadata.get("state_generation").clone();
            let now_ns = (now * 1e9).to_u64().ok_or(Error::Range)?;
            let bytes = wire::state_at(cereal, now_ns, || u128::from(now_ns))?;
            let message = serialize::read_message(&mut &bytes[..], ReaderOptions::new())?;
            let event = message.get_root::<event::Reader<'_>>()?;
            let openpilot_cereal::log_capnp::event::CarrotNavi(navi) =
                event.which().map_err(capnp::Error::from)?
            else {
                return Err(Error::Range);
            };
            self.navi = crate::live::navigation::parse_carrot_navi(navi?.into(), now, None);
        }
        if metadata.get("media_generation") != &self.media_generation {
            self.media_generation = metadata.get("media_generation").clone();
            self.update_media(snapshot)?;
        }
        self.apply_results();
        self.map_age = None;
        self.stalled = false;
        if let Some(record) = self
            .last_records
            .get("render:map_main")
            .filter(|record| connected && record.get("present").truth())
        {
            let received = record
                .get("received_at_ms")
                .int()?
                .to_i128()
                .ok_or(Error::Range)?;
            self.map_age =
                Some(i64::try_from((wall_ms - received).max(0)).map_err(|_| Error::Range)?);
            self.stalled = self.map_age.is_some_and(|age| age > 3000);
            if self.stalled {
                self.media.remove("render:map_main");
            }
        }
        let mut state = self.simulator.update(&SimulatorInput::default(), dt);
        state.speed_kph = self.navi.as_ref().map_or(0.0, |navi| {
            navi.vehicle.as_ref().map_or_else(
                || navi.speed.as_ref().map_or(0.0, |speed| speed.current_kph),
                |vehicle| vehicle.speed_kph,
            )
        });
        state.speed_limit_kph = self
            .navi
            .as_ref()
            .and_then(|navi| navi.speed.as_ref())
            .and_then(|speed| speed.road_limit_kph);
        state.speed_limit_source = state.speed_limit_kph.map(|_| "n".into());
        state.onroad = connected;
        state.navi_live.clone_from(&self.navi);
        state.navi_dashboard = Some(self.dashboard(metadata, wall_ms)?);
        state.center_clock_text = Some(clock.into());
        Ok(state)
    }

    fn clear_media(&mut self) -> Result<(), Error> {
        self.media.clear();
        self.epoch = self.epoch.checked_add(1).ok_or(Error::Range)?;
        self.requested.clear();
        self.worker.reset();
        self.map_age = None;
        self.stalled = false;
        Ok(())
    }

    pub fn close(&mut self) -> bool {
        self.worker.close()
    }
}

fn integer(value: &Value) -> Result<i64, Error> {
    value.int()?.to_i64().ok_or(Error::Range)
}
fn unsigned(value: &Value) -> Result<u64, Error> {
    value.int()?.to_u64().ok_or(Error::Range)
}
fn text(value: &Value) -> Result<Option<String>, Error> {
    if matches!(value, Value::Null) {
        Ok(None)
    } else {
        Ok(Some(value.string()?))
    }
}
