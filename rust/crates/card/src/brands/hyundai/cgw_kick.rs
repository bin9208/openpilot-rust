//! `ClassicCgwBlinker`: the common indicator requests (ATC turns, navigation
//! lane changes, manual/model indicators, Bluetooth taps) on a classic-CAN
//! Hyundai through the BCM's own CGW1 (0x541).
//!
//! Port of joongyu01/carrot-wip-dev's classic CGW1 kick (b6fd849d3,
//! cf9b7fd5f, 4ffd69d78, 0c411803f, b4f6450c4): the stock frame with the
//! lever side (byte 5 bits 1..2) and that side's lamp bit flashing 0.35 s
//! on / 0.35 s off like stock, every 60 ms; panda holds it and sends it right
//! behind the next stock CGW1. Measured there: the Palisade lit its exterior
//! lamps (10-27), the K5 DL3 only the cluster and BSM (11-14). Whether a car
//! lights its lamps is only known after the parked test.
//!
//! An opt-in test switch, off by default: card asks for panda's
//! `CLASSIC_CGW_BLINKER` safety bit only when `ClassicCgwBlinker` is on and
//! the fingerprint shows a classic-CAN car with an 8-byte CGW1 on bus 0
//! (parameters.rs). Every other car never runs any of this.
//!
//! The request side follows the CAN-FD Kick (`kick.rs`): the policy worker in
//! Kick mode, the shared indicator lease and the lamp-ownership journal with
//! the modeld isolation mask before any driving frame.
use super::Error;
use crate::blink::{
    owner::Owner,
    source::{record_ownership, Feedback, Mode, Record, Worker},
    Side,
};
use openpilot_can::{Frame, Packet};
use openpilot_cereal::car_capnp::car_state;
use openpilot_messaging::state::State as Subscribers;
use openpilot_params::Params;
use serde_json::json;
use std::collections::VecDeque;

pub const CGW1: u32 = 0x541;
/// Same as joongyu's `CGW_TX_FRAMES` (6 frames at 100 Hz); panda's own limit is 50 ms.
const TX_PERIOD: f64 = 0.06;
/// Stock CGW1 lamp rhythm (Palisade 00000e40, IONIQ PHEV 0000000f seg 9).
const FLASH_HALF: f64 = 0.35;
/// The stock frame we copy; panda wants it within 250 ms.
const STOCK_TTL: f64 = 0.2;
/// panda: 30 s per continuous request, then 1 s with no request at all.
const TRIAL: f64 = 29.5;
const REARM: f64 = 1.05;
/// `ClassicCgwBlinkerTest` 1/2: one parked 3 s run left/right.
const TEST_LENGTH: f64 = 3.0;
pub const TEST_KEY: &str = "ClassicCgwBlinkerTest";

fn fresh(stamp: f64, now: f64, ttl: f64) -> bool {
    stamp.is_finite() && (0. ..ttl).contains(&(now - stamp))
}

fn side_code(side: Side) -> u8 {
    match side {
        Side::Left => 1,
        Side::Right => 2,
    }
}

/// The stock frame with the lever side (1 left, 2 right) and, when `lamp`,
/// that side's lamp bit (CF_Gway_TurnSigLh byte 2 bit 3, CF_Gway_TurnSigRh
/// byte 7 bit 6). The same bytes panda's `hyundai_cgw_fits` admits.
pub fn cgw_frame(stock: &[u8; 8], side: u8, lamp: bool) -> Frame {
    let mut data = *stock;
    if side == 1 {
        data[5] |= 0x04;
        if lamp {
            data[2] |= 0x08;
        }
    } else {
        data[5] |= 0x02;
        if lamp {
            data[7] |= 0x40;
        }
    }
    Frame {
        address: CGW1,
        bus: 0,
        data: data.to_vec(),
    }
}

/// No hazard switch (byte 4 bits 1..2) and no driver lever (byte 5 bits 1..2).
fn idle(stock: &[u8; 8]) -> bool {
    stock[4] & 0x06 == 0 && stock[5] & 0x06 == 0
}

/// The stock lamps themselves are dark (the BCM never shows our lamp).
fn dark(stock: &[u8; 8]) -> bool {
    stock[2] & 0x18 == 0 && stock[7] & 0xc0 == 0
}

pub struct CgwKick {
    /// `CLASSIC_CGW_BLINKER` in this car's safety config.
    pub configured: bool,
    owner: Owner,
    stock: Option<[u8; 8]>,
    stock_at: f64,
    worker: Option<Worker>,
    record: Option<Record>,
    seen: VecDeque<String>,
    request_id: Option<String>,
    request_side: u8,
    accepted_id: Option<String>,
    last_sent: f64,
    first_sent: Option<f64>,
    request_started: f64,
    tap_until: f64,
    tapping: bool,
    ready: bool,
    output_ready: bool,
    next_update: f64,
    feedback: Feedback,
    feedback_at: f64,
    last_journal: f64,
    journal_side: u8,
    // Settings, refreshed every 0.5 s.
    settings_at: f64,
    setting: bool,
    onroad: bool,
    test_setting: i32,
    // Output timing (panda's trial mirrored so every frame we send is admitted).
    pulse_side: u8,
    pulse_started: f64,
    last_tx: f64,
    trial_started: f64,
    test_side: u8,
    test_started: f64,
    test_armed: bool,
    pub sent: u32,
    /// The direction this cycle's request asks for (1 left, 2 right, 0).
    pub direction: u8,
    pub status: &'static str,
}

impl CgwKick {
    pub fn new(configured: bool) -> Self {
        Self {
            configured,
            owner: Owner::new("Classic CGW1"),
            stock: None,
            stock_at: f64::NEG_INFINITY,
            worker: None,
            record: None,
            seen: VecDeque::with_capacity(256),
            request_id: None,
            request_side: 0,
            accepted_id: None,
            last_sent: -1.,
            first_sent: None,
            request_started: 0.,
            tap_until: 0.,
            tapping: false,
            ready: false,
            output_ready: false,
            next_update: 0.,
            feedback: Feedback::default(),
            feedback_at: 0.,
            last_journal: f64::NEG_INFINITY,
            journal_side: 0,
            settings_at: f64::NEG_INFINITY,
            setting: false,
            onroad: false,
            test_setting: 0,
            pulse_side: 0,
            pulse_started: 0.,
            last_tx: f64::NEG_INFINITY,
            trial_started: 0.,
            test_side: 0,
            test_started: 0.,
            test_armed: true,
            sent: 0,
            direction: 0,
            status: "idle",
        }
    }

    /// Switching `ClassicCgwBlinker` off stops at once; on waits for the next
    /// start (the panda flag is set at fingerprint).
    pub fn refresh(&mut self, settings: &Params, now: f64) {
        if now - self.settings_at < 0.5 {
            return;
        }
        self.settings_at = now;
        self.setting = settings.get_bool("ClassicCgwBlinker").unwrap_or(false);
        self.onroad = settings.get_bool("IsOnroad").unwrap_or(false);
        self.test_setting = super::parameters::setting_int(settings, TEST_KEY).unwrap_or(0);
    }

    fn enabled(&self) -> bool {
        self.configured && self.setting && self.onroad
    }

    /// The newest stock CGW1 on bus 0 (our own frames come back as 128+).
    pub fn observe(&mut self, packets: &[Packet], now: f64) {
        if !self.configured {
            return;
        }
        for frame in packets.iter().flat_map(|p| &p.frames) {
            if frame.bus == 0 && frame.address == CGW1 {
                self.stock = <[u8; 8]>::try_from(frame.data.as_slice()).ok();
                self.stock_at = now;
            }
        }
    }

    fn stock_ready(&self, now: f64) -> Option<[u8; 8]> {
        self.stock.filter(|s| fresh(self.stock_at, now, STOCK_TTL) && idle(s))
    }

    fn driver_active(&self, now: f64) -> bool {
        self.stock
            .is_some_and(|s| fresh(self.stock_at, now, STOCK_TTL) && !idle(&s))
    }

    pub fn update(&mut self, valid: bool, now: f64) -> bool {
        let enabled = self.enabled() && valid;
        if !enabled || now >= self.next_update {
            self.owner.acquire(enabled, now);
            self.next_update = now + 0.05;
        }
        self.output_ready = enabled && self.owner.held && self.stock_ready(now).is_some();
        self.output_ready
    }

    /// carrot Kick `request`: the policy worker's fresh hold or tap as a side
    /// (1 left, 2 right) while healthy.
    pub fn request(
        &mut self,
        cs: car_state::Reader<'_>,
        now: f64,
        initialized: bool,
        subs: &Subscribers,
        settings: &Params,
    ) -> Result<u8, Error> {
        let enabled = self.enabled();
        if !enabled {
            self.worker = None;
            self.record = None;
        } else if initialized && self.worker.is_none() {
            self.worker = Some(Worker::new(Mode::Kick, String::new(), false, false));
        }
        if let Some(worker) = &mut self.worker {
            if worker
                .update(now, cs, true, subs, settings, &self.feedback)
                .is_err()
            {
                worker.record = None;
            }
            self.record.clone_from(&worker.record);
        }
        let gear = cs
            .get_gear_shifter()
            .unwrap_or(car_state::GearShifter::Unknown);
        let (left, right) = (cs.get_left_blinker(), cs.get_right_blinker());
        self.ready = enabled
            && self.output_ready
            && initialized
            && cs.get_can_valid()
            && gear == car_state::GearShifter::Drive
            && !(left && right);
        let record = self.record.as_ref();
        let healthy = self.ready && record.is_some_and(|r| fresh(r.stamp, now, 0.2) && r.ok);
        let hold = record.and_then(|r| r.hold.as_ref());
        let candidate = hold
            .map(|h| (h.id.clone(), h.direction, h.time))
            .or_else(|| {
                record.and_then(|r| {
                    let start = r.events.len().saturating_sub(4);
                    r.events[start..]
                        .iter()
                        .rev()
                        .find(|e| fresh(e.time, now, 0.4))
                        .map(|e| (e.id.clone(), e.direction, e.time))
                })
            });
        let valid_request = candidate
            .as_ref()
            .filter(|(id, _, _)| !id.is_empty() && id.len() <= 100);
        if let Some(current) = self.request_id.clone() {
            let continuing = if self.tapping {
                now < self.tap_until
            } else {
                valid_request.is_some_and(|(id, side, _)| {
                    *id == current && side_code(*side) == self.request_side
                })
            };
            let opposite = if self.request_side == 1 { right } else { left };
            if !healthy || !continuing || opposite || now - self.request_started >= 300. {
                self.request_id = None;
                self.request_side = 0;
                self.accepted_id = None;
            }
        }
        if let Some((id, side, time)) = valid_request {
            if !self.seen.contains(id) {
                if self.seen.len() == 256 {
                    self.seen.pop_front();
                }
                self.seen.push_back(id.clone());
                if healthy
                    && self.request_id.is_none()
                    && fresh(*time, now, 0.4)
                    && !(left || right)
                {
                    self.request_id = Some(id.clone());
                    self.request_side = side_code(*side);
                    self.request_started = now;
                    self.tap_until = now + 1.6;
                    self.first_sent = None;
                    self.tapping = hold.is_none();
                }
            }
        }
        self.direction = if healthy { self.request_side } else { 0 };
        Ok(self.direction)
    }

    /// One frame every 60 ms inside panda's trial; `None` between them.
    fn emit(&mut self, stock: &[u8; 8], side: u8, now: f64) -> Option<Frame> {
        if side != self.pulse_side {
            self.pulse_side = side;
            self.pulse_started = now;
        }
        if now - self.last_tx >= REARM {
            self.trial_started = now; // panda rearmed: a new trial opens
        } else if now - self.trial_started >= TRIAL {
            return None; // trial spent: stay quiet until panda rearms, then go on
        }
        if now - self.last_tx < TX_PERIOD - 0.005 {
            return None;
        }
        self.last_tx = now;
        let lamp = (((now - self.pulse_started) / FLASH_HALF) as i64) % 2 == 0;
        Some(cgw_frame(stock, side, lamp))
    }

    /// The frames for this cycle: the parked test, else the driving request
    /// once modeld isolated that lamp side.
    pub fn frames(
        &mut self,
        cs: car_state::Reader<'_>,
        engaged: bool,
        isolation_mask: Option<u8>,
        now: f64,
        settings: &Params,
    ) -> Vec<Frame> {
        if !self.configured {
            return Vec::new();
        }
        let stock = self.stock_ready(now);
        // Parked test: P, wheels still, not engaged, stock lamps dark.
        let parked = cs
            .get_gear_shifter()
            .unwrap_or(car_state::GearShifter::Unknown)
            == car_state::GearShifter::Park
            && cs.get_v_ego() == 0.
            && !engaged;
        if !matches!(self.test_setting, 1 | 2) {
            self.test_armed = true;
            self.test_side = 0;
        } else if self.test_armed {
            self.test_armed = false;
            let ok = self.enabled() && parked && stock.is_some_and(|s| dark(&s));
            self.status = if ok { "test_running" } else { "test_refused" };
            if ok {
                self.test_side = self.test_setting as u8;
                self.test_started = now;
                self.sent = 0;
            } else {
                let _ = settings.put(TEST_KEY, b"0");
            }
        }
        if self.test_side != 0 {
            let running = now - self.test_started < TEST_LENGTH;
            match stock.filter(|_| running && parked && self.enabled()) {
                Some(s) => {
                    return self
                        .emit(&s, self.test_side, now)
                        .map(|f| {
                            self.sent += 1;
                            vec![f]
                        })
                        .unwrap_or_default();
                }
                None => {
                    self.status = if running { "test_stopped" } else { "test_done" };
                    self.test_side = 0;
                    self.pulse_side = 0;
                    let _ = settings.put(TEST_KEY, b"0");
                    return Vec::new();
                }
            }
        }
        let side = self.direction;
        let Some(stock) = stock.filter(|_| matches!(side, 1 | 2) && self.enabled()) else {
            if self.pulse_side != 0 {
                self.status = "drive_end";
            }
            self.pulse_side = 0;
            return Vec::new();
        };
        // Our lamp must never read back as the driver's lever: journal the
        // ownership and send only after modeld isolated that side.
        if (side != self.journal_side || now - self.last_journal >= 0.1)
            && record_ownership(
                now,
                &json!({"time": now, "direction": side, "source": "classic_cgw"}),
            )
            .is_ok()
        {
            self.journal_side = side;
            self.last_journal = now;
        }
        if isolation_mask.is_none_or(|mask| mask & side == 0) {
            self.status = "drive_isolating";
            return Vec::new();
        }
        if self.pulse_side != side {
            self.sent = 0;
        }
        self.status = "drive_running";
        self.emit(&stock, side, now)
            .map(|f| {
                self.sent += 1;
                vec![f]
            })
            .unwrap_or_default()
    }

    /// carrot Kick `feedback`: acknowledge submitted frames to the worker.
    pub fn feedback(&mut self, now: f64, frames: &[Frame]) {
        let sent = self.request_id.is_some()
            && frames.iter().any(|f| {
                f.address == CGW1
                    && f.data.len() == 8
                    && match self.request_side {
                        1 => f.data[5] & 0x04 != 0,
                        2 => f.data[5] & 0x02 != 0,
                        _ => false,
                    }
            });
        if sent {
            if self.first_sent.is_none() {
                self.first_sent = Some(now);
            }
            if self.tapping && self.accepted_id != self.request_id {
                self.tap_until = now + 1.;
            }
            self.accepted_id.clone_from(&self.request_id);
            self.last_sent = now;
        } else if !fresh(self.last_sent, now, 0.2) {
            self.accepted_id = None;
        }
        if self.worker.is_some() && now >= self.feedback_at {
            let driver = self.driver_active(now);
            let accepted = if self.ready {
                self.accepted_id.clone()
            } else {
                None
            };
            self.feedback = Feedback {
                stamp: now,
                state: if driver {
                    "driver_override"
                } else if self.ready {
                    "ready"
                } else {
                    "waiting"
                },
                available: self.ready,
                first_sent: if accepted.is_some() {
                    self.first_sent
                } else {
                    None
                },
                accepted_id: accepted,
                lamps: None,
            };
            self.feedback_at = now + 0.05;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDLE: [u8; 8] = [0x03, 0x04, 0x41, 0x80, 0xc0, 0x00, 0x00, 0x04];

    #[test]
    fn frame_bits_match_the_stock_lever_and_lamp() {
        // IONIQ PHEV 0000000f seg 9: right lever = byte 5 0x02, right lamp = byte 7 0x40.
        assert_eq!(
            cgw_frame(&IDLE, 2, true).data,
            [0x03, 0x04, 0x41, 0x80, 0xc0, 0x02, 0x00, 0x44]
        );
        assert_eq!(
            cgw_frame(&IDLE, 2, false).data,
            [0x03, 0x04, 0x41, 0x80, 0xc0, 0x02, 0x00, 0x04]
        );
        assert_eq!(
            cgw_frame(&IDLE, 1, true).data,
            [0x03, 0x04, 0x49, 0x80, 0xc0, 0x04, 0x00, 0x04]
        );
    }

    #[test]
    fn driver_lever_or_hazard_is_not_idle() {
        assert!(idle(&IDLE));
        let mut lever = IDLE;
        lever[5] |= 0x02;
        assert!(!idle(&lever));
        let mut hazard = IDLE;
        hazard[4] |= 0x02;
        assert!(!idle(&hazard));
    }

    #[test]
    fn emits_every_60_ms_and_flashes_at_the_stock_rhythm() {
        let mut k = CgwKick::new(true);
        let mut lamps = Vec::new();
        let mut now = 10.;
        for _ in 0..140 {
            if let Some(f) = k.emit(&IDLE, 2, now) {
                lamps.push((now, f.data[7] & 0x40 != 0));
            }
            now += 0.01;
        }
        // 1.4 s at one frame per 60 ms
        assert!((22..=24).contains(&lamps.len()), "{}", lamps.len());
        assert!(lamps.windows(2).all(|w| w[1].0 - w[0].0 >= 0.055));
        assert!(lamps[0].1, "starts lit");
        let changes = lamps.windows(2).filter(|w| w[0].1 != w[1].1).count();
        assert_eq!(changes, 3, "0.35 s halves over 1.4 s");
    }

    #[test]
    fn a_long_hold_pauses_for_panda_to_rearm_then_continues() {
        let mut k = CgwKick::new(true);
        let mut now = 0.;
        let mut times = Vec::new();
        while now < 33. {
            if k.emit(&IDLE, 1, now).is_some() {
                times.push(now);
            }
            now += 0.01;
        }
        let gap = times
            .windows(2)
            .map(|w| w[1] - w[0])
            .fold(0., f64::max);
        assert!(gap >= REARM - 0.011, "a quiet gap of at least 1 s: {gap}");
        let first_gap = times.windows(2).position(|w| w[1] - w[0] > 0.5).unwrap();
        assert!(times[first_gap] - times[0] < 30., "inside panda's 30 s trial");
        assert!(*times.last().unwrap() > 31., "continues after the rearm");
    }
}
