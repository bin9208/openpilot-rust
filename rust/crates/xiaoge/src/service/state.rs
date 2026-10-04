use crate::{
    config::Config,
    lane::ResultData,
    settings::Settings,
    vision::{Detection, Gate, Side},
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub enum Stream {
    Wide,
    Road,
}

impl Stream {
    pub const fn index(self) -> usize {
        match self {
            Self::Wide => 0,
            Self::Road => 1,
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Wide => "wide",
            Self::Road => "road",
        }
    }
}

pub struct Camera {
    pub error: String,
    pub last_frame: f64,
    pub snapshot_request: u64,
    pub snapshot_response: u64,
    pub jpeg: Option<Arc<[u8]>>,
}

impl Camera {
    pub fn available(&self, now: f64) -> bool {
        self.last_frame != 0.0 && now - self.last_frame <= 2.0 && self.error.is_empty()
    }
}

pub struct Model {
    pub path: String,
    pub loaded: bool,
    pub error: String,
}

pub struct Metrics {
    pub last_inference: f64,
    pub latency_ms: f64,
    pub thread_cpu_ms: f64,
    pub count: u64,
    pub fps: f64,
    pub window_start: f64,
    pub window_count: u32,
}

impl Metrics {
    pub const fn new(now: f64) -> Self {
        Self {
            last_inference: 0.0,
            latency_ms: 0.0,
            thread_cpu_ms: 0.0,
            count: 0,
            fps: 0.0,
            window_start: now,
            window_count: 0,
        }
    }

    pub fn record(&mut self, timing: Timing) -> Result<(), crate::Error> {
        self.last_inference = timing.received;
        self.latency_ms = (timing.finished - timing.started) * 1000.0;
        self.thread_cpu_ms = timing.thread_cpu_ms;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(crate::Error::Invalid("inference count overflow"))?;
        self.window_count = self
            .window_count
            .checked_add(1)
            .ok_or(crate::Error::Invalid("inference window count overflow"))?;
        if timing.finished - self.window_start >= 1.0 {
            self.fps = super::rounded(
                f64::from(self.window_count) / (timing.finished - self.window_start),
                1,
            )
            .map_err(|_| crate::Error::Invalid("inference rate rounding failed"))?;
            self.window_start = timing.finished;
            self.window_count = 0;
        }
        Ok(())
    }
}

pub struct Timing {
    pub received: f64,
    pub started: f64,
    pub finished: f64,
    pub thread_cpu_ms: f64,
}

pub struct State {
    pub config: Config,
    pub config_generation: u64,
    pub settings: Settings,
    pub last_param_refresh: f64,
    pub cameras: [Camera; 2],
    pub models: [Model; 2],
    pub metrics: [Metrics; 2],
    pub gate: Gate,
    pub sides: [Detection; 2],
    pub blindspot_side: Option<Side>,
    pub blindspot_active: [bool; 2],
    pub blindspot_updated: u64,
    pub lane: ResultData,
    pub lane_updated: u64,
    pub side_at: [f64; 2],
    pub followup_until: f64,
}

impl State {
    pub fn new(config: Config, models: [Model; 2], now: f64) -> Self {
        Self {
            config,
            config_generation: 0,
            settings: Settings::default(),
            last_param_refresh: 0.0,
            cameras: ["waiting for wide road camera", "waiting for road camera"].map(|error| {
                Camera {
                    error: error.to_owned(),
                    last_frame: 0.0,
                    snapshot_request: 0,
                    snapshot_response: 0,
                    jpeg: None,
                }
            }),
            models,
            metrics: [Metrics::new(now), Metrics::new(now)],
            gate: Gate {
                active: false,
                side: None,
                reason: "waiting for carState and modelV2",
                lane_width: 0.0,
            },
            sides: [Detection::default(); 2],
            blindspot_side: None,
            blindspot_active: [false; 2],
            blindspot_updated: 0,
            lane: ResultData::failed(String::new()),
            lane_updated: 0,
            side_at: [0.0; 2],
            followup_until: 0.0,
        }
    }

    pub fn configure(&mut self, config: Config) -> Result<(), crate::Error> {
        self.config_generation = self
            .config_generation
            .checked_add(1)
            .ok_or(crate::Error::Invalid("configuration generation overflow"))?;
        self.config = config;
        self.sides = [Detection::default(); 2];
        Ok(())
    }

    pub fn lane_fresh(&self, now: f64, nanos: u64) -> bool {
        self.cameras[1].available(now)
            && self.models[1].loaded
            && self.lane.valid
            && crate::vision::fresh(self.lane_updated, nanos, crate::vision::LANE_TIMEOUT_NS)
    }

    pub fn side_valid(&self, side: Side, now: f64, nanos: u64) -> bool {
        self.cameras[0].available(now)
            && self.models[0].loaded
            && self.gate.active
            && self.gate.side == Some(side)
            && self.blindspot_side == Some(side)
            && crate::vision::fresh(
                self.blindspot_updated,
                nanos,
                crate::vision::BLINDSPOT_TIMEOUT_NS,
            )
    }
}
