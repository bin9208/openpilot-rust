use super::{catalog, decoder, request::Request, worker::DecodeWorker};
use crate::models::{NaviDashboardState, NaviItemStatus, NaviLiveState, NaviMediaFrame};
use capnp::{message::ReaderOptions, serialize};
use num_traits::ToPrimitive;
use openpilot_cereal::{custom_capnp::carrot_navi_media, log_capnp::event};
use openpilot_msgq::Subscriber;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Transport(#[from] openpilot_msgq::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error("navigation media clock/counter cannot be represented")]
    Range,
}
pub struct MediaInput {
    socket: Subscriber,
    session: String,
    media: BTreeMap<String, Arc<NaviMediaFrame>>,
    items: BTreeMap<String, NaviItemStatus>,
    epoch: u64,
    configs: BTreeMap<String, (u64, Vec<u8>)>,
    worker: DecodeWorker,
    last_received: f64,
    map_received: f64,
    received: i64,
    error: Option<String>,
}
impl MediaInput {
    /// # Errors
    /// Returns a native media subscription initialization error.
    pub fn new() -> Result<Self, Error> {
        Self::open(false)
    }
    /// # Errors
    /// Returns an isolated media subscription initialization error.
    pub fn isolated() -> Result<Self, Error> {
        Self::open(true)
    }
    fn open(isolated: bool) -> Result<Self, Error> {
        let capacity = openpilot_messaging::services::lookup("carrotNaviMedia")
            .expect("original media service")
            .queue_size;
        let socket = if isolated {
            Subscriber::with_capacity("carrotNaviMedia", false, capacity)
        } else {
            Subscriber::for_runtime("carrotNaviMedia", false, capacity)
        }?;
        Ok(Self {
            socket,
            session: String::new(),
            media: BTreeMap::new(),
            items: BTreeMap::new(),
            epoch: 0,
            configs: BTreeMap::new(),
            worker: DecodeWorker::new(decoder::factory()),
            last_received: 0.0,
            map_received: 0.0,
            received: 0,
            error: None,
        })
    }
    /// # Errors
    /// Returns transport errors or an unrepresentable timestamp/counter.
    pub fn update_at(
        &mut self,
        navi: Option<&NaviLiveState>,
        now: f64,
    ) -> Result<NaviDashboardState, Error> {
        while let Some(bytes) = self.socket.receive(Duration::ZERO)? {
            if let Err(error) = self.consume(&bytes, now) {
                self.error = Some(error.to_string().chars().take(256).collect());
            }
        }
        for result in self.worker.poll() {
            let frame = &result.frame;
            if result.epoch != self.epoch
                || self
                    .media
                    .get(&frame.key)
                    .is_some_and(|current| frame.sequence <= current.sequence)
            {
                continue;
            }
            if frame.key == "render:map_main" {
                self.map_received = now;
            }
            self.media.insert(frame.key.clone(), result.frame);
        }
        if self.session.is_empty() {
            if let Some(navi) = navi {
                self.session.clone_from(&navi.session_id);
            }
        }
        let connected =
            navi.is_some() || (self.last_received > 0.0 && now - self.last_received <= 3.0);
        let map_age = age(now, self.map_received)?;
        let map_stalled = connected && map_age.is_some_and(|age| age > 3000);
        if map_stalled {
            self.media.remove("render:map_main");
        }
        if !connected && self.last_received > 0.0 {
            self.clear();
        }
        let items = catalog::waiting()
            .into_iter()
            .map(|waiting| self.items.get(&waiting.key).cloned().unwrap_or(waiting))
            .collect();
        Ok(NaviDashboardState {
            connected,
            endpoint: "ipc://carrotNaviMedia".into(),
            session_id: self.session.clone(),
            received_count: self.received,
            last_received_age_ms: age(now, self.last_received)?,
            map_frame_age_ms: map_age,
            map_stream_stalled: map_stalled,
            error: self.error.clone(),
            media: self.media.values().map(|frame| (**frame).clone()).collect(),
            items,
            ..NaviDashboardState::default()
        })
    }
    fn consume(&mut self, bytes: &[u8], now: f64) -> Result<(), Error> {
        let message = serialize::read_message(&mut &bytes[..], ReaderOptions::new())?;
        let event = message.get_root::<event::Reader<'_>>()?;
        let openpilot_cereal::log_capnp::event::CarrotNaviMedia(data) =
            event.which().map_err(capnp::Error::from)?
        else {
            return Ok(());
        };
        let data = data?;
        self.message(data, now)
    }
    fn message(&mut self, data: carrot_navi_media::Reader<'_>, now: f64) -> Result<(), Error> {
        if data.get_schema_version() != 1 {
            return Ok(());
        }
        let session = data.get_session_id()?.to_str()?;
        if !session.is_empty() && session != self.session {
            self.clear();
            self.session = session.into();
        }
        let kind = data.get_kind()?.to_str()?;
        let name = data.get_name()?.to_str()?;
        if !matches!(kind, "image" | "render") || name.is_empty() {
            return Ok(());
        }
        let key = format!("{kind}:{name}");
        let sequence = data.get_sequence();
        let present = data.get_present();
        let reason = data.get_reason()?.to_str()?;
        let reason = (!reason.is_empty()).then(|| reason.into());
        self.items.insert(
            key.clone(),
            NaviItemStatus {
                key: key.clone(),
                sequence,
                present,
                reason: reason.clone(),
            },
        );
        self.last_received = now;
        self.received = self.received.checked_add(1).ok_or(Error::Range)?;
        let kind = data.get_message_type();
        if !present || kind == 4 {
            self.worker.discard(&key);
            self.media.insert(
                key.clone(),
                Arc::new(NaviMediaFrame {
                    key: key.clone(),
                    sequence,
                    present: false,
                    reason,
                    ..NaviMediaFrame::default()
                }),
            );
            if key == "render:map_main" {
                self.map_received = 0.0;
            }
            return Ok(());
        }
        let width = i64::from(data.get_width());
        let height = i64::from(data.get_height());
        let payload = data.get_payload()?;
        match kind {
            1 => {
                self.worker.discard(&key);
                self.media.insert(
                    key.clone(),
                    Arc::new(NaviMediaFrame {
                        key: key.clone(),
                        sequence,
                        present: true,
                        mime: if data.get_format_or_reason() == 1 {
                            "image/png"
                        } else {
                            "image/jpeg"
                        }
                        .into(),
                        width,
                        height,
                        data: Some(Arc::from(payload)),
                        ..NaviMediaFrame::default()
                    }),
                );
                if key == "render:map_main" {
                    self.map_received = now;
                }
            }
            2 => {
                self.configs.insert(key, (sequence, payload.to_vec()));
            }
            3 => {
                let config = self.configs.get(&key);
                self.worker.submit(Request {
                    epoch: self.epoch,
                    key,
                    sequence,
                    payload: payload.to_vec(),
                    config: config.map_or_else(Vec::new, |value| value.1.clone()),
                    keyframe: data.get_flags() & 1 != 0,
                    width,
                    height,
                    reset: false,
                });
            }
            _ => {}
        }
        Ok(())
    }
    fn clear(&mut self) {
        self.media.clear();
        self.epoch = self.epoch.checked_add(1).expect("media epoch overflow");
        self.worker.reset();
        self.configs.clear();
        self.map_received = 0.0;
    }
    pub fn close(&mut self) -> bool {
        self.clear();
        self.worker.close()
    }
    #[must_use]
    pub fn dropped_decode_requests(&self) -> u64 {
        self.worker.dropped_requests()
    }
}
fn age(now: f64, received: f64) -> Result<Option<i64>, Error> {
    if received <= 0.0 {
        return Ok(None);
    }
    ((now - received).max(0.0) * 1000.0)
        .trunc()
        .to_i64()
        .map(Some)
        .ok_or(Error::Range)
}
