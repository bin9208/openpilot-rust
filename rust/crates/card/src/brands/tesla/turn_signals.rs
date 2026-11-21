//! Tesla lamp execution adapted from xpilot's bounded DAS_bodyControls
//! sessions. Port of carrot's `selfdrive/car/tesla_turn_signals.py`.
use openpilot_can::{Frame, Packet};

pub const BODY: u32 = 0x3e9;
pub const UI: u32 = 0x311;
pub const FRONT: u32 = 0x3f5;
const TEMPLATE_NS: u64 = 1_500_000_000;
const CONTEXT_NS: u64 = 200_000_000;
const FEEDBACK_NS: u64 = 500_000_000;
const FEEDBACK_DEGRADED_NS: u64 = 1_500_000_000;
const FRONT_FAULT_GRACE_NS: u64 = 500_000_000;
const ECHO_NS: u64 = 1_200_000_000;
const SESSION_NS: u64 = 60_000_000_000;
const CANCEL_NS: u64 = 5_000_000_000;
const OFF_STABLE_NS: u64 = 500_000_000;
const ON_NS: u64 = 2_500_000_000;

fn age_ok(now: u64, t: u64, limit: u64) -> bool {
    now >= t && now - t <= limit
}

pub fn body_checksum(data: &[u8]) -> u8 {
    (0xe9u32 + 3 + data[..7].iter().map(|b| u32::from(*b)).sum::<u32>()) as u8
}

pub fn body_frame(template: &[u8], direction: u8) -> Option<Vec<u8>> {
    if !matches!(direction, 1..=3)
        || template.len() != 8
        || template[1] & 3 != 0
        || body_checksum(template) != template[7]
    {
        return None;
    }
    let mut data = template.to_vec();
    data[1] = (data[1] & 0xfc) | direction;
    data[2] = (data[2] & 0xe1) | ((if direction == 3 { 4 } else { 8 }) << 1);
    data[6] = (data[6] & 15) | ((((data[6] >> 4) + 1) & 15) << 4);
    data[7] = body_checksum(&data);
    Some(data)
}

#[derive(Clone, Debug, Default)]
struct Session {
    request_id: u64,
    direction: u8,
    started: u64,
    sent: u32,
    awaiting: Option<Vec<u8>>,
    submitted: u64,
    cancelling: bool,
    cancel_started: u64,
    cancel_attempts: u32,
    cancel_echo: bool,
    active_echoes: u32,
    cancel_echoes: u32,
    rejected: u32,
    manual: bool,
    on_seen: bool,
    off_since: Option<u64>,
    last_on: u64,
    last_front_on: u64,
    lost_since: Option<u64>,
    turn: bool,
    lamp_only: bool,
}

/// The carState fields the session reads.
#[derive(Clone, Copy, Debug, Default)]
pub struct Car {
    pub driver_valid: bool,
    pub driver_direction: u8,
    pub stalk_counts: [u8; 2],
    pub cruise_cancel: bool,
    pub can_valid: bool,
    pub sensors_invalid: bool,
    pub drive: bool,
    pub v_ego: f64,
    pub trailer: bool,
    pub steer_fault: bool,
    pub standstill: bool,
}

pub struct Request {
    pub lat_active: bool,
    /// modelV2.meta autoTurnSignalDirection / RequestId (0 when stale).
    pub direction: u8,
    pub request_id: u64,
    /// meta.desire is the turn desire for `direction`.
    pub turn_desire: bool,
    pub fresh: bool,
    pub turn_hold_allowed: bool,
    pub lamp_intent: Option<(u8, u64)>,
}

pub struct TurnSignals {
    pub configured: bool,
    session: Option<Session>,
    template: Option<Vec<u8>>,
    template_time: u64,
    template_consumed_time: u64,
    ui: (u8, u8),
    front: (u8, u8),
    front_fault_since: [Option<u64>; 2],
    ui_time: u64,
    front_time: u64,
    context_time: u64,
    blocked_id: u64,
    disabled_status: String,
    pub status: String,
    last_stalk_counts: Option<[u8; 2]>,
    failed: bool,
    driver_idle_time: u64,
    driver_press_time: u64,
}

impl TurnSignals {
    pub fn new(configured: bool, disabled_reason: &str) -> Self {
        Self {
            configured,
            session: None,
            template: None,
            template_time: 0,
            template_consumed_time: 0,
            ui: (0, 0),
            front: (3, 3),
            front_fault_since: [None, None],
            ui_time: 0,
            front_time: 0,
            context_time: 0,
            blocked_id: 0,
            disabled_status: disabled_reason.into(),
            status: if configured {
                "idle".into()
            } else {
                disabled_reason.into()
            },
            last_stalk_counts: None,
            failed: false,
            driver_idle_time: 0,
            driver_press_time: 0,
        }
    }

    pub fn session_lamp_only(&self) -> Option<bool> {
        self.session.as_ref().map(|s| s.lamp_only)
    }

    pub fn owned_direction(&self) -> u8 {
        self.session
            .as_ref()
            .filter(|s| !s.manual)
            .map_or(0, |s| s.direction)
    }

    fn front_fault_expired(&self, now: u64) -> bool {
        self.front_fault_since
            .iter()
            .any(|t| t.is_some_and(|t| now >= t && now - t >= FRONT_FAULT_GRACE_NS))
    }

    fn feedback_fresh(&self, now: u64) -> bool {
        [self.ui_time, self.front_time]
            .iter()
            .all(|t| *t > 0 && age_ok(now, *t, FEEDBACK_NS))
    }

    fn feedback_available(&self, now: u64) -> bool {
        // Startup feedback may lag TX, but must confirm lamps before its deadline.
        let times = [self.ui_time, self.front_time];
        self.feedback_fresh(now)
            || (self
                .session
                .as_ref()
                .is_some_and(|s| s.on_seen || s.active_echoes > 0)
                && times
                    .iter()
                    .all(|t| *t > 0 && age_ok(now, *t, FEEDBACK_DEGRADED_NS))
                && times.iter().any(|t| age_ok(now, *t, FEEDBACK_NS)))
    }

    fn off(&self, now: u64) -> bool {
        self.feedback_fresh(now) && self.ui == (0, 0) && self.front == (0, 0)
    }

    pub fn ready_request_id(&self, now: u64) -> u64 {
        let Some(s) = &self.session else {
            return 0;
        };
        if self.failed
            || s.cancelling
            || s.manual
            || s.active_echoes == 0
            || !s.on_seen
            || self.context_time == 0
            || !age_ok(now, self.context_time, CONTEXT_NS)
            || !(now >= s.started && now - s.started < SESSION_NS)
            || !self.feedback_available(now)
            || self.front_fault_expired(now)
            || !(now >= s.last_on && now - s.last_on < ON_NS)
            || !(now >= s.last_front_on && now - s.last_front_on < ON_NS + FEEDBACK_DEGRADED_NS)
        {
            return 0;
        }
        s.request_id
    }

    fn cancel(&mut self, now: u64, reason: &str) {
        let Some(s) = &mut self.session else {
            return;
        };
        if s.cancelling {
            return;
        }
        self.blocked_id = s.request_id;
        if s.sent == 0 {
            self.session = None;
            self.status = "cancelled_before_send".into();
            return;
        }
        s.cancelling = true;
        s.cancel_started = now;
        self.status = format!("cancelling:{reason}");
    }

    fn fail(&mut self, reason: &str) {
        self.failed = true;
        self.status = format!("fault:{reason}");
        if let Some(s) = &self.session {
            self.blocked_id = s.request_id;
        }
    }

    fn handoff(&mut self, now: u64) {
        if self.session.is_none() {
            return;
        }
        // CANCEL may extinguish a human's signal: defer cleanup until fresh
        // physical idle and a stable, actual lamp OFF.
        if let Some(s) = &mut self.session {
            s.manual = true;
        }
        if self.failed {
            return;
        }
        self.cancel(now, "driver");
        if let Some(s) = &mut self.session {
            s.cancel_started = 0;
            s.off_since = None;
            self.status = "driver_takeover:waiting_off".into();
        }
    }

    pub fn observe_driver(&mut self, car: &Car, now: u64) {
        if car.driver_valid
            && self
                .last_stalk_counts
                .is_some_and(|c| c != car.stalk_counts)
        {
            self.driver_press_time = now;
        }
        self.last_stalk_counts = Some(car.stalk_counts);
        let driver = car.driver_valid && car.driver_direction != 0;
        let pressed =
            self.driver_press_time != 0 && age_ok(now, self.driver_press_time, CONTEXT_NS);
        self.driver_idle_time = if car.driver_valid && !(driver || pressed) {
            now
        } else {
            0
        };
        if driver || pressed {
            self.handoff(now);
        }
    }

    pub fn observe(&mut self, packets: &[Packet], now: u64) {
        if !self.configured {
            return;
        }
        for packet in packets {
            let timestamp = packet.mono_time;
            if !(0 < timestamp && timestamp <= now) {
                continue;
            }
            for frame in &packet.frames {
                self.observe_frame(frame, timestamp, now);
            }
        }
        self.advance(now);
    }

    fn observe_frame(&mut self, frame: &Frame, timestamp: u64, now: u64) {
        let (address, data, source) = (frame.address, frame.data.as_slice(), frame.bus);
        if source == 1 && address == BODY {
            if data.len() == 8
                && body_checksum(data) == data[7]
                && data[1] & 3 == 0
                && timestamp >= self.template_time
                && timestamp > self.template_consumed_time
            {
                self.template = Some(data.to_vec());
                self.template_time = timestamp;
            }
        } else if source == 0 && address == UI && data.len() == 7 && timestamp >= self.ui_time {
            // UI_warning: Motorola left at bit 25, Intel right at bit 26.
            let checksum =
                (0x11u32 + 3 + data[1..].iter().map(|b| u32::from(*b)).sum::<u32>()) as u8;
            if data[0] == checksum {
                self.ui = (data[3] & 3, (data[3] >> 2) & 3);
                self.ui_time = timestamp;
                if [self.ui.0, self.ui.1].iter().all(|v| matches!(v, 1 | 2)) {
                    self.handoff(now);
                }
                if self.ui.0 == 3 || self.ui.1 == 3 {
                    self.cancel(now, "invalid_lamp_feedback");
                }
            }
        } else if source == 1 && address == FRONT && data.len() == 8 && timestamp >= self.front_time
        {
            if self.front_fault_expired(timestamp) {
                self.cancel(now, "invalid_lamp_feedback");
            }
            self.front = ((data[6] >> 2) & 3, (data[6] >> 4) & 3);
            self.front_time = timestamp;
            // Recorded lamps alternate 2/1: debounce 2, never use it as ON/OFF.
            for (side, value) in [self.front.0, self.front.1].into_iter().enumerate() {
                if value != 2 {
                    self.front_fault_since[side] = None;
                } else if self.front_fault_since[side].is_none() {
                    self.front_fault_since[side] = Some(timestamp);
                }
            }
            if self.front.0 == 3 || self.front.1 == 3 {
                self.cancel(now, "invalid_lamp_feedback");
            }
        }
        let mut echo = None;
        if let Some(s) = &mut self.session {
            if address == BODY
                && s.awaiting.as_deref() == Some(data)
                && matches!(source, 0x81 | 0xc1)
            {
                s.awaiting = None;
                echo = Some(source);
            }
        }
        if let Some(source) = echo {
            if source == 0xc1 {
                let cancelling = self.session.as_ref().is_some_and(|s| s.cancelling);
                if let Some(s) = &mut self.session {
                    s.rejected += 1;
                }
                if cancelling {
                    self.fail("panda_rejected_cancel");
                } else {
                    self.cancel(now, "panda_rejected");
                }
            } else if let Some(s) = &mut self.session {
                if data[1] & 3 == 3 {
                    s.cancel_echoes += 1;
                    s.cancel_echo = s.cancelling;
                } else {
                    s.active_echoes += 1;
                }
            }
        }
        let side = |pair: (u8, u8), index: usize| if index == 0 { pair.0 } else { pair.1 };
        if let Some(direction) = self.session.as_ref().map(|s| s.direction) {
            let other = usize::from(2 - direction);
            let mine = usize::from(direction - 1);
            if (0 < self.ui_time
                && self.ui_time <= now
                && now - self.ui_time <= FEEDBACK_NS
                && matches!(side(self.ui, other), 1 | 2))
                || (0 < self.front_time
                    && self.front_time <= now
                    && now - self.front_time <= FEEDBACK_NS
                    && side(self.front, other) == 1)
            {
                self.handoff(now);
            }
            let fresh = self.feedback_fresh(now);
            let (ui, front, ui_time, front_time) =
                (self.ui, self.front, self.ui_time, self.front_time);
            if let Some(s) = &mut self.session {
                if fresh && matches!(side(ui, mine), 1 | 2) && side(front, mine) == 1 {
                    s.on_seen = true;
                    s.last_on = s.last_on.max(ui_time.min(front_time));
                }
                if side(front, mine) == 1 && age_ok(now, front_time, FEEDBACK_NS) {
                    s.last_front_on = s.last_front_on.max(front_time);
                }
                if s.on_seen && !fresh {
                    if side(ui, mine) == 2 && age_ok(now, ui_time, FEEDBACK_NS) {
                        s.last_on = s.last_on.max(ui_time);
                    }
                    if side(front, mine) == 1 && age_ok(now, front_time, FEEDBACK_NS) {
                        s.last_on = s.last_on.max(front_time);
                    }
                }
            }
        }
    }

    pub fn update(&mut self, car: &Car, request: &Request, now: u64) -> Vec<Frame> {
        self.context_time = if request.fresh { now } else { 0 };
        self.observe_driver(car, now);
        let pressed =
            self.driver_press_time != 0 && age_ok(now, self.driver_press_time, CONTEXT_NS);
        let driver = car.driver_valid && car.driver_direction != 0;
        let hazards = matches!(self.ui.0, 1 | 2) && matches!(self.ui.1, 1 | 2);
        if hazards {
            self.handoff(now);
        }
        let mut direction = if request.fresh { request.direction } else { 0 };
        let mut request_id = if request.fresh { request.request_id } else { 0 };
        let lamp_only = request.fresh
            && direction == 0
            && request.lamp_intent.is_some_and(|(d, id)| {
                matches!(d, 1 | 2) && (1u64 << 31) < id && id < (1u64 << 32)
            })
            && car.v_ego.is_finite()
            && car.v_ego >= 30. / 3.6
            && !car.trailer
            && !car.steer_fault;
        if lamp_only {
            if let Some((d, id)) = request.lamp_intent {
                direction = d;
                request_id = id;
            }
        }
        let stopped_hold = self.session.as_ref().is_some_and(|s| {
            s.turn
                && s.on_seen
                && s.active_echoes > 0
                && !s.cancelling
                && !s.manual
                && request_id == s.request_id
                && direction == s.direction
        }) && request.turn_hold_allowed
            && car.standstill
            && car.v_ego.is_finite()
            && car.v_ego.abs() < 0.1;
        let checks: [(bool, &str); 10] = [
            (self.configured, ""),
            (request.fresh, "context_stale"),
            (!car.cruise_cancel, "cruise_cancel"),
            (
                request.lat_active || stopped_hold || lamp_only,
                "lateral_inactive",
            ),
            (car.can_valid, "can_invalid"),
            (!car.sensors_invalid, "vehicle_sensors_invalid"),
            (car.drive, "not_in_drive"),
            (car.driver_valid, "driver_input_unavailable"),
            (!driver && !pressed, "driver"),
            (!hazards, "hazards"),
        ];
        let blocked = checks.iter().find(|(valid, _)| !valid).map(|(_, reason)| {
            if reason.is_empty() {
                self.disabled_status.clone()
            } else {
                (*reason).to_owned()
            }
        });
        if let Some(blocked) = blocked {
            self.cancel(now, &blocked);
            if self.session.is_none() && !self.failed {
                self.status = if self.configured {
                    format!("blocked:{blocked}")
                } else {
                    self.disabled_status.clone()
                };
            }
        } else if !matches!(direction, 1 | 2) || request_id == 0 {
            self.cancel(now, "intent_inactive");
            if self.session.is_none()
                && (self.status.starts_with("blocked:") || self.status.starts_with("waiting:"))
                && !self.failed
            {
                self.status = "idle".into();
            }
        } else if let Some(s) = &self.session {
            if request_id != s.request_id || direction != s.direction || lamp_only != s.lamp_only {
                self.cancel(now, "request_changed");
            }
        } else if !self.failed && request_id != self.blocked_id {
            let fresh = |t: u64| 0 < t && t <= now && now - t <= FEEDBACK_NS;
            if !fresh(self.ui_time) {
                self.status = "waiting:ui_feedback".into();
            } else if !fresh(self.front_time) {
                self.status = "waiting:front_feedback".into();
            } else if self.ui.0 == 3
                || self.ui.1 == 3
                || matches!(self.front.0, 2 | 3)
                || matches!(self.front.1, 2 | 3)
            {
                self.status = "waiting:invalid_lamp_feedback".into();
            } else if self.ui != (0, 0) || self.front != (0, 0) {
                self.status = "waiting:lamps_off".into();
            } else if self.template.is_none() || !age_ok(now, self.template_time, TEMPLATE_NS) {
                self.status = "waiting:body_template".into();
            } else {
                self.session = Some(Session {
                    request_id,
                    direction,
                    started: now,
                    lamp_only,
                    turn: !lamp_only && request.turn_desire,
                    ..Session::default()
                });
                self.status = "active".into();
            }
        }
        self.advance(now);
        self.take_sends(now, false)
    }

    fn advance(&mut self, now: u64) {
        let Some(s) = self.session.clone() else {
            return;
        };
        if self.failed {
            // Keep provenance after an unconfirmed cancel until actually off.
            let off = self.off(now);
            if let Some(s) = &mut self.session {
                if off {
                    match s.off_since {
                        None => s.off_since = Some(now),
                        Some(since) if now - since >= OFF_STABLE_NS => self.session = None,
                        _ => {}
                    }
                } else {
                    s.off_since = None;
                }
            }
            return;
        }
        if s.manual {
            let idle = self.driver_idle_time != 0 && age_ok(now, self.driver_idle_time, CONTEXT_NS);
            let off = self.off(now);
            let Some(s) = &mut self.session else { return };
            if !idle || !off {
                s.off_since = None;
                return;
            }
            let since = *s.off_since.get_or_insert(now);
            if now - since < OFF_STABLE_NS {
                return;
            }
            if s.cancel_started == 0 {
                s.cancel_started = now;
                self.status = "cancelling:driver_cleanup".into();
            }
            if s.awaiting.is_some() && now - s.submitted >= ECHO_NS {
                s.awaiting = None;
            }
        }
        let s = self.session.clone().expect("session");
        if !s.cancelling {
            if self.context_time == 0 || !age_ok(now, self.context_time, CONTEXT_NS) {
                self.cancel(now, "context_stale");
            } else if now - s.started >= SESSION_NS {
                self.cancel(now, "session_timeout");
            } else if !self.feedback_available(now) {
                self.cancel(now, "feedback_stale");
            } else if self.front_fault_expired(now) {
                self.cancel(now, "invalid_lamp_feedback");
            } else if !s.on_seen && now - s.started >= ON_NS {
                self.cancel(now, "lamp_not_confirmed");
            } else if s.on_seen {
                let off = self.off(now);
                let mut lost = false;
                if let Some(s) = &mut self.session {
                    if off {
                        let since = *s.lost_since.get_or_insert(now);
                        lost = now - since >= OFF_STABLE_NS;
                    } else {
                        s.lost_since = None;
                    }
                }
                if lost {
                    self.cancel(now, "lamps_lost");
                }
                if now.saturating_sub(s.last_on) >= ON_NS {
                    self.cancel(now, "lamp_feedback_lost");
                }
                // UI activity alone must never hide a physical channel stuck OFF.
                if now.saturating_sub(s.last_front_on) >= ON_NS + FEEDBACK_DEGRADED_NS {
                    self.cancel(now, "physical_lamp_feedback_lost");
                }
            }
        }
        let timed_out = self
            .session
            .as_ref()
            .is_some_and(|s| s.awaiting.is_some() && now - s.submitted >= ECHO_NS);
        if timed_out {
            if let Some(s) = &mut self.session {
                s.awaiting = None;
            }
            self.cancel(now, "tx_echo_timeout");
        }
        let off = self.off(now);
        let mut done = false;
        let mut cancel_timeout = false;
        if let Some(s) = &mut self.session {
            if s.cancelling {
                if s.cancel_echo && off {
                    let since = *s.off_since.get_or_insert(now);
                    done = now - since >= OFF_STABLE_NS;
                } else {
                    s.off_since = None;
                }
                cancel_timeout = !done && now - s.cancel_started >= CANCEL_NS;
            }
        }
        if done {
            self.status = "cancelled".into();
            self.session = None;
        } else if cancel_timeout {
            self.fail("cancel_not_confirmed");
        }
    }

    pub fn take_sends(&mut self, now: u64, cancel_only: bool) -> Vec<Frame> {
        let template_ok = self.template.is_some() && age_ok(now, self.template_time, TEMPLATE_NS);
        let idle_ok = self.driver_idle_time != 0 && age_ok(now, self.driver_idle_time, CONTEXT_NS);
        let off = self.off(now);
        let failed = self.failed;
        let Some(s) = &mut self.session else {
            return Vec::new();
        };
        if failed
            || s.awaiting.is_some()
            || s.cancel_echo
            || (s.manual && (s.cancel_started == 0 || !idle_ok || !off))
            || (cancel_only && !s.cancelling)
            || !template_ok
        {
            return Vec::new();
        }
        if s.cancelling && s.cancel_attempts >= 3 {
            return Vec::new();
        }
        let Some(data) = self
            .template
            .as_deref()
            .and_then(|t| body_frame(t, if s.cancelling { 3 } else { s.direction }))
        else {
            return Vec::new();
        };
        self.template_consumed_time = self.template_time;
        self.template = None;
        s.awaiting = Some(data.clone());
        s.submitted = now;
        s.sent += 1;
        s.cancel_attempts += u32::from(s.cancelling);
        vec![Frame {
            address: BODY,
            data,
            bus: 1,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> Vec<u8> {
        let mut data = vec![0, 0, 0, 0, 0, 0, 0x10, 0];
        data[7] = body_checksum(&data);
        data
    }

    #[test]
    fn body_frame_sets_direction_and_counter() {
        let frame = body_frame(&template(), 1).unwrap();
        assert_eq!(frame[1] & 3, 1);
        assert_eq!(frame[6] >> 4, 2);
        assert_eq!(frame[7], body_checksum(&frame));
        assert!(
            body_frame(&frame, 2).is_none(),
            "non-idle template rejected"
        );
    }

    #[test]
    fn disabled_reports_reason_and_never_sends() {
        let mut signals = TurnSignals::new(false, "disabled:setting_off");
        let sends = signals.update(
            &Car::default(),
            &Request {
                lat_active: true,
                direction: 1,
                request_id: 5,
                turn_desire: false,
                fresh: true,
                turn_hold_allowed: false,
                lamp_intent: None,
            },
            1_000,
        );
        assert!(sends.is_empty());
        assert_eq!(signals.status, "disabled:setting_off");
    }
}
