use crate::{
    compute::ComputeMode,
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
    last_finished: f64,
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
            last_finished: 0.0,
            latency_ms: 0.0,
            thread_cpu_ms: 0.0,
            count: 0,
            fps: 0.0,
            window_start: now,
            window_count: 0,
        }
    }

    /// Keep normal start cadence, but release CPU for an interval after an overrun.
    /// The original receive time remains available for status/freshness consumers.
    pub fn ready(&self, now: f64, interval: f64) -> bool {
        if self.count == 0 {
            return true;
        }
        let previous = if self.latency_ms >= interval * 1000.0 {
            self.last_finished
        } else {
            self.last_inference
        };
        now - previous >= interval
    }

    pub fn record(&mut self, timing: Timing) -> Result<(), crate::Error> {
        self.last_inference = timing.received;
        self.last_finished = timing.finished;
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
    pub compute_mode: ComputeMode,
    pub compute_generation: u64,
    pub compute_since_ns: u64,
    pub compute_status: [Option<openpilot_logging::Fields>; 2],
    pub last_param_refresh: f64,
    pub cameras: [Camera; 2],
    pub models: [Model; 2],
    pub metrics: [Metrics; 2],
    pub gate: Gate,
    pub sides: [Detection; 2],
    pub blindspot_side: Option<Side>,
    pub blindspot_active: [bool; 2],
    pub blindspot_updated: u64,
    pub blindspot_side_updated: [u64; 2],
    pub next_side: Side,
    pub lane: ResultData,
    pub lane_updated: u64,
    pub side_at: [f64; 2],
    pub followup_until: [f64; 2],
    /// `OnnxBsdEnabled`: visual blindspot detection is off unless it is 1.
    pub bsd_enabled: bool,
    /// Bumped on every enable/disable so an inference that raced the switch
    /// can be recognised and dropped.
    pub bsd_generation: u64,
}

impl State {
    /// carrot's `_refresh_settings_locked`: a change of the enable switch
    /// clears every published blindspot result and the inference pacing.
    pub fn set_bsd_enabled(&mut self, enabled: bool) -> bool {
        if enabled == self.bsd_enabled {
            return false;
        }
        self.bsd_generation += 1;
        self.clear_blindspot(0);
        self.side_at = [0.0; 2];
        self.followup_until = [0.0; 2];
        self.metrics[0].last_inference = 0.0;
        self.bsd_enabled = enabled;
        true
    }

    /// No side active, no side selected; `updated` stamps the cleared result.
    pub fn clear_blindspot(&mut self, updated: u64) {
        self.blindspot_active = [false; 2];
        self.blindspot_side = None;
        self.blindspot_updated = updated;
        self.blindspot_side_updated = [0; 2];
    }

    pub fn new(config: Config, models: [Model; 2], now: f64) -> Self {
        Self {
            config,
            config_generation: 0,
            settings: Settings::default(),
            compute_mode: ComputeMode::Cpu,
            compute_generation: 0,
            compute_since_ns: (now.max(0.0) * 1e9) as u64,
            compute_status: [None, None],
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
                eligible_sides: Vec::new(),
                lane_widths: [0.0; 2],
            },
            sides: [Detection::default(); 2],
            blindspot_side: None,
            blindspot_active: [false; 2],
            blindspot_updated: 0,
            blindspot_side_updated: [0; 2],
            next_side: Side::Left,
            lane: ResultData::failed(String::new()),
            lane_updated: 0,
            side_at: [0.0; 2],
            followup_until: [0.0; 2],
            bsd_enabled: false,
            bsd_generation: 0,
        }
    }

    pub fn select_compute(&mut self, mode: ComputeMode) -> Result<(), crate::Error> {
        if self.compute_mode == mode {
            return Ok(());
        }
        self.compute_generation = self
            .compute_generation
            .checked_add(1)
            .ok_or(crate::Error::Invalid("compute generation overflow"))?;
        self.compute_mode = mode;
        self.compute_status = [None, None];
        self.lane = ResultData::failed("compute mode changed; waiting for a new result".into());
        self.lane_updated = 0;
        self.blindspot_updated = 0;
        self.blindspot_side_updated = [0; 2];
        self.blindspot_active = [false; 2];
        self.sides = [Detection::default(); 2];
        Ok(())
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
        self.bsd_enabled
            && self.cameras[0].available(now)
            && self.models[0].loaded
            && self.gate.active
            && self.gate.eligible_sides.contains(&side)
            && crate::vision::fresh(
                self.blindspot_side_updated[side.index()],
                nanos,
                crate::vision::BLINDSPOT_TIMEOUT_NS,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A state whose left side would be valid and active if detection is on.
    fn detecting() -> State {
        let model = |loaded| Model {
            path: String::new(),
            loaded,
            error: String::new(),
        };
        let mut state = State::new(Config::default(), [model(true), model(true)], 0.0);
        state.cameras[0].last_frame = 10.0;
        state.cameras[0].error.clear();
        state.gate.active = true;
        state.gate.eligible_sides = vec![Side::Left];
        state
    }
    fn mark_left(state: &mut State) {
        state.blindspot_active[0] = true;
        state.blindspot_side = Some(Side::Left);
        state.blindspot_updated = 10_000_000_000;
        state.blindspot_side_updated[0] = 10_000_000_000;
        state.side_at[0] = 10.0;
        state.followup_until[0] = 11.5;
    }

    #[test]
    fn visual_blindspot_is_off_until_enabled_and_a_switch_clears_results() {
        let mut state = detecting();
        mark_left(&mut state);
        // Off by default: a result is never reported valid.
        assert!(!state.bsd_enabled);
        assert!(!state.side_valid(Side::Left, 10.0, 10_000_000_000));

        assert!(state.set_bsd_enabled(true));
        assert_eq!(state.bsd_generation, 1);
        assert_eq!(state.blindspot_active, [false; 2]);
        assert_eq!(state.blindspot_side_updated, [0; 2]);
        assert_eq!(state.side_at, [0.0; 2]);
        assert_eq!(state.followup_until, [0.0; 2]);

        mark_left(&mut state);
        assert!(state.side_valid(Side::Left, 10.0, 10_000_000_000));
        // The same value again changes nothing.
        assert!(!state.set_bsd_enabled(true));
        assert_eq!(state.bsd_generation, 1);
        assert!(state.blindspot_active[0]);

        assert!(state.set_bsd_enabled(false));
        assert_eq!(state.bsd_generation, 2);
        assert_eq!(state.blindspot_active, [false; 2]);
        assert_eq!(state.blindspot_side, None);
        assert_eq!(state.blindspot_updated, 0);
        assert!(!state.side_valid(Side::Left, 10.0, 10_000_000_000));
    }

    #[test]
    fn status_reports_the_switch_and_hides_the_gate_while_off() {
        let mut state = detecting();
        let text = |state: &State| {
            state
                .status(10.0, 10_000_000_000)
                .unwrap()
                .to_json()
                .unwrap()
        };
        let off = text(&state);
        assert!(off.contains("\"bsdEnabled\": false"), "{off}");
        assert!(off.contains("visual BSD is disabled"), "{off}");
        state.set_bsd_enabled(true);
        let on = text(&state);
        assert!(on.contains("\"bsdEnabled\": true"), "{on}");
        assert!(!on.contains("visual BSD is disabled"), "{on}");
    }
}
