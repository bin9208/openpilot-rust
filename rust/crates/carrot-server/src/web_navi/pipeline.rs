use super::{
    fmp4::{Muxer, Sample},
    media::Media,
    media_stats::Stats,
    wire,
};
use crate::{Error, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct Pipeline {
    muxer: Muxer,
    images: BTreeMap<String, Vec<u8>>,
    pub initialization: Option<Vec<u8>>,
    gop: VecDeque<Vec<u8>>,
    active: bool,
    retry_at: Option<Instant>,
    error: String,
    session: String,
    stats: Stats,
}
pub(super) struct Packet {
    pub is_map: bool,
    pub wire: Vec<u8>,
}
impl Pipeline {
    pub fn status(&self) -> Value {
        let pipeline = Value::object([
            (
                "mode",
                Value::text(if self.active {
                    "server-fmp4"
                } else {
                    "server-fmp4-waiting"
                }),
            ),
            ("available", Value::Bool(true)),
            ("active", Value::Bool(self.active)),
            (
                "initializationPresent",
                Value::Bool(self.initialization.is_some()),
            ),
            ("gopSegments", Value::integer(self.gop.len())),
            ("error", Value::text(&self.error)),
        ]);
        self.stats
            .status(pipeline, self.initialization.is_some(), self.gop.len())
    }
    pub fn reset(&mut self, clear_config: bool) {
        if clear_config {
            self.muxer.clear();
            self.error.clear();
        } else {
            self.muxer.close();
        }
        self.initialization = None;
        self.gop.clear();
        self.active = false;
        self.retry_at = None;
    }
    pub fn clear(&mut self) {
        self.images.clear();
        self.reset(true);
        self.session.clear();
        self.stats = Stats::default();
    }
    pub fn bootstrap(&self, include_map: bool) -> Vec<Vec<u8>> {
        if self
            .stats
            .map_at
            .is_none_or(|at| at.elapsed() > Duration::from_secs(3))
        {
            return Vec::new();
        }
        let mut bootstrap = Vec::new();
        if include_map && self.active {
            if let Some(init) = &self.initialization {
                bootstrap.push(init.clone());
                bootstrap.extend(self.gop.iter().cloned());
            }
        }
        bootstrap.extend(self.images.values().cloned());
        bootstrap
    }
    pub fn receive(&mut self, media: Media, map_requested: bool) -> Result<Vec<Packet>, Error> {
        if media.session != self.session {
            self.session.clone_from(&media.session);
            self.images.clear();
            self.reset(true);
            self.stats = Stats::default();
        }
        self.stats.remember(&media);
        let key = format!("{}:{}", media.kind, media.name);
        if !media.present || media.message_type == 4 {
            self.images.remove(&key);
        } else if media.message_type == 1 {
            self.images.insert(key, media.raw_wire.clone());
        }
        if !media.is_map() {
            return Ok(vec![Packet {
                is_map: false,
                wire: media.raw_wire,
            }]);
        }
        if !map_requested {
            return Ok(Vec::new());
        }
        if !media.present || media.message_type == 4 {
            self.reset(true);
            let reason = media.metadata.get("reason").string()?;
            return Ok(vec![Self::cleared(&media.metadata, &reason)?]);
        }
        if media.message_type == 2 {
            if self
                .muxer
                .configure(&media.payload, media.dimensions, &media.session)
            {
                self.initialization = None;
                self.gop.clear();
                self.active = false;
                self.retry_at = None;
                self.error.clear();
            }
            return Ok(Vec::new());
        }
        if media.message_type != 3 || media.payload.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(retry) = self.retry_at {
            if !media.keyframe() || Instant::now() < retry {
                return Ok(Vec::new());
            }
            self.retry_at = None;
        }
        let output = match self.muxer.push(Sample {
            payload: &media.payload,
            sequence: media.sequence,
            timestamp_ms: media.timestamp_ms,
            keyframe: media.keyframe(),
        }) {
            Ok(output) => output,
            Err(error) => {
                self.retry_at = Some(Instant::now() + Duration::from_secs(5));
                self.error = error.chars().take(256).collect();
                self.initialization = None;
                self.gop.clear();
                self.active = false;
                return Ok(vec![Self::cleared(&media.metadata, "server_remux_error")?]);
            }
        };
        let mut packets = Vec::new();
        if let Some(init) = output.initialization {
            let mut metadata = media.metadata.clone();
            wire::update(
                &mut metadata,
                [
                    ("type", Value::text("carrotNaviFmp4")),
                    ("kind", Value::text("fmp4")),
                    ("messageType", Value::integer(2)),
                    ("flags", Value::integer(0)),
                    ("width", Value::integer(init.width)),
                    ("height", Value::integer(init.height)),
                    ("mime", Value::text(&init.mime)),
                    ("frameCount", Value::integer(0)),
                    ("keyframeCount", Value::integer(0)),
                    ("durationMs", Value::integer(0)),
                ],
            );
            let wire = wire::frame(&metadata, &init.payload)?;
            self.initialization = Some(wire.clone());
            self.gop.clear();
            self.active = true;
            self.error.clear();
            packets.push(Packet { is_map: true, wire });
        }
        for segment in output.segments {
            let mut metadata = media.metadata.clone();
            wire::update(
                &mut metadata,
                [
                    ("type", Value::text("carrotNaviFmp4")),
                    ("kind", Value::text("fmp4")),
                    ("sequence", Value::integer(segment.sequence)),
                    (
                        "sourceTimestampMillis",
                        Value::integer(segment.timestamp_ms),
                    ),
                    ("messageType", Value::integer(3)),
                    ("flags", Value::integer(u8::from(segment.keyframe))),
                    ("frameCount", Value::integer(1)),
                    ("keyframeCount", Value::integer(u8::from(segment.keyframe))),
                    ("durationMs", Value::integer(segment.duration_ms)),
                ],
            );
            let wire = wire::frame(&metadata, &segment.payload)?;
            if segment.keyframe {
                self.gop.clear();
            }
            if self.gop.len() == 90 {
                self.gop.pop_front();
            }
            self.gop.push_back(wire.clone());
            packets.push(Packet { is_map: true, wire });
        }
        Ok(packets)
    }
    fn cleared(metadata: &Value, reason: &str) -> Result<Packet, Error> {
        let mut metadata = metadata.clone();
        let reason = if reason.is_empty() { "cleared" } else { reason };
        wire::update(
            &mut metadata,
            [
                ("type", Value::text("carrotNaviFmp4")),
                ("kind", Value::text("fmp4")),
                ("present", Value::Bool(false)),
                ("messageType", Value::integer(4)),
                ("flags", Value::integer(0)),
                (
                    "reason",
                    Value::text(&reason.chars().take(64).collect::<String>()),
                ),
                ("frameCount", Value::integer(0)),
                ("keyframeCount", Value::integer(0)),
                ("durationMs", Value::integer(0)),
            ],
        );
        Ok(Packet {
            is_map: true,
            wire: wire::frame(&metadata, &[])?,
        })
    }
}
