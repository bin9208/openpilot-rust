//! Pure indicator policy; never requests steering or changes ATC admission.
//! Port of carrot's `tools/jetlink/blink_policy.py`.
use super::{
    turn_journey::{JourneyInput, TurnJourney},
    turn_location::{offset, Position, Target, TurnLocation},
    Side,
};
use std::collections::VecDeque;

const NEAR_STOP: f64 = 5. / 3.6;
const MAX_HOLD: f64 = 300.;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Hold {
    pub id: String,
    pub direction: Side,
    pub source: String,
    pub time: f64,
    pub device: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Command {
    pub id: String,
    pub time: f64,
    pub direction: Side,
    pub device: Option<String>,
}

/// An automatic explanation for a lamp: `("lane" | "turn" | ..., side)`.
pub type Automatic = (String, Side);

fn session_id() -> String {
    let mut bytes = [0u8; 16];
    if std::io::Read::read_exact(
        &mut match std::fs::File::open("/dev/urandom") {
            Ok(file) => file,
            Err(_) => return "0".repeat(32),
        },
        &mut bytes,
    )
    .is_err()
    {
        return "0".repeat(32);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub struct Policy {
    session: String,
    serial: u64,
    seen: VecDeque<String>,
    pub hold: Option<Hold>,
    taps: Vec<Command>,
    auto_previous: Option<Automatic>,
    auto_armed: bool,
    pub suppressed: bool,
    mode: i32,
    turned: bool,
    centered: Option<f64>,
    started: f64,
    off_since: Option<f64>,
    pub relights: u32,
}

pub struct PolicyInput<'a> {
    pub now: f64,
    pub mode: i32,
    pub ok: bool,
    pub commands: &'a [Command],
    pub speed: f64,
    pub angle: f64,
    pub gear: &'a str,
    pub remote: bool,
    pub automatic: Option<Automatic>,
    pub auto_valid: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self::new()
    }
}

impl Policy {
    pub fn new() -> Self {
        Self {
            session: session_id(),
            serial: 0,
            seen: VecDeque::with_capacity(256),
            hold: None,
            taps: Vec::new(),
            auto_previous: None,
            auto_armed: false,
            suppressed: false,
            mode: 0,
            turned: false,
            centered: None,
            started: 0.,
            off_since: None,
            relights: 0,
        }
    }

    fn start(&mut self, direction: Side, source: &str, now: f64) {
        self.serial += 1;
        self.hold = Some(Hold {
            id: format!("{}:{}", self.session, self.serial),
            direction,
            source: source.to_owned(),
            time: now,
            device: None,
        });
        self.turned = false;
        self.centered = None;
        self.started = now;
        self.off_since = None;
        self.relights = 0;
    }

    /// One recovery tap after a completed receiver send, never after a
    /// rejected send or a cancel.
    pub fn relight(
        &mut self,
        now: f64,
        activated_id: Option<&str>,
        acknowledged: bool,
        lamps: (bool, bool),
        retry_unlit: bool,
    ) -> bool {
        let Some(h) = self.hold.clone() else {
            self.off_since = None;
            return false;
        };
        if !matches!(h.source.as_str(), "turn" | "observation" | "tbt_display")
            || (activated_id != Some(h.id.as_str()) && !retry_unlit)
            || !acknowledged
            || self.relights != 0
            || lamps.0
            || lamps.1
        {
            self.off_since = None;
            return false;
        }
        let since = *self.off_since.get_or_insert(now);
        // Sustained absence, not a normal dark half-cycle of the lamps.
        if now - since < 1.5 || now - self.started >= MAX_HOLD {
            return false;
        }
        self.serial += 1;
        self.hold = Some(Hold {
            id: format!("{}:{}", self.session, self.serial),
            time: now,
            ..h
        });
        self.relights += 1;
        self.off_since = None;
        true
    }

    /// A rejected start must not become an invisible five-minute toggle: the
    /// next physical press is a new request.
    pub fn acknowledged(
        &mut self,
        now: f64,
        activated_id: Option<&str>,
        driver_override: bool,
    ) -> Option<&Hold> {
        if let Some(h) = &self.hold {
            if driver_override || (now - h.time > 4. && activated_id != Some(h.id.as_str())) {
                self.hold = None;
            }
        }
        self.hold.as_ref()
    }

    pub fn update(&mut self, i: PolicyInput<'_>) -> (Vec<Command>, Option<Hold>) {
        // Changing the setting or reconnecting never joins a running maneuver.
        if i.mode != self.mode || !i.ok {
            self.hold = None;
            self.auto_armed = false;
            self.suppressed = i.automatic.is_some();
            self.mode = i.mode;
        }
        self.taps.retain(|e| (0. ..0.4).contains(&(i.now - e.time)));
        if !i.ok {
            self.taps.clear();
            return (Vec::new(), None);
        }
        let threshold = if i.automatic.as_ref().is_some_and(|a| a.0 == "lane") {
            2
        } else {
            1
        };
        let eligible = if i.mode != 0 && i.mode >= threshold {
            i.automatic.clone()
        } else {
            None
        };
        if !i.auto_valid {
            self.auto_armed = false;
            self.suppressed = true;
        }
        if eligible.is_none() && i.auto_valid {
            self.auto_armed = true;
            self.suppressed = false;
        }
        let drivable = matches!(i.gear, "park" | "neutral" | "drive");
        if let Some(h) = &mut self.hold {
            if h.source != "bluetooth" && i.auto_valid {
                if let Some(e) = &eligible {
                    if e.1 == h.direction {
                        // The same maneuver gaining another explanation is not another tap.
                        h.source.clone_from(&e.0);
                    }
                }
            }
            let remove = i.mode == 0
                || i.now - self.started >= MAX_HOLD
                || !drivable
                || (h.source == "bluetooth" && !i.remote)
                || (h.source == "bluetooth" && !i.angle.is_finite())
                || (h.source != "bluetooth"
                    && (!i.auto_valid || eligible != Some((h.source.clone(), h.direction))));
            if remove {
                self.hold = None;
            }
        }
        let mut manual = false;
        for event in i.commands {
            if self.seen.contains(&event.id) {
                continue;
            }
            if self.seen.len() == 256 {
                self.seen.pop_front();
            }
            self.seen.push_back(event.id.clone());
            manual = true;
            self.suppressed = eligible.is_some();
            let previous = self.hold.take();
            if previous
                .as_ref()
                .is_some_and(|p| p.direction == event.direction)
            {
                // The same button cancels any held signal, including an ATC one.
                continue;
            }
            if i.mode != 0
                && i.angle.is_finite()
                && drivable
                && (i.speed.abs() <= NEAR_STOP
                    || previous.as_ref().is_some_and(|p| p.source == "bluetooth"))
            {
                self.start(event.direction, "bluetooth", event.time);
                if let Some(h) = &mut self.hold {
                    h.device.clone_from(&event.device);
                }
            } else {
                self.taps.push(event.clone());
            }
        }
        if !manual && self.hold.is_none() && self.auto_armed && !self.suppressed {
            if let Some(e) = &eligible {
                if Some(e) != self.auto_previous.as_ref() {
                    let (source, side) = e.clone();
                    self.start(side, &source, i.now);
                }
            }
        }
        self.auto_previous = eligible;
        if let Some(h) = &self.hold {
            if h.source == "bluetooth" {
                // Physical completion: steer into the selected turn, then
                // return near centre while moving.
                let signed = i.angle * h.direction.sign();
                self.turned |= signed >= 30. && i.speed.abs() >= 1. / 3.6;
                if self.turned && i.angle.abs() <= 10. && i.speed.abs() >= 1. / 3.6 {
                    let since = *self.centered.get_or_insert(i.now);
                    if i.now - since >= 0.4 {
                        self.hold = None;
                    }
                } else {
                    self.centered = None;
                }
            }
        }
        let start = self.taps.len().saturating_sub(4);
        (self.taps[start..].to_vec(), self.hold.clone())
    }
}

/// Start once at the audio turn cue; finish on physical turn completion,
/// not on desire loss.
pub struct AutomaticSignals {
    cue: Option<f64>,
    pub direction: Option<Side>,
    turned: bool,
    centered: Option<f64>,
    mode: Option<i32>,
    heading: Option<f64>,
    rotation: f64,
    location: TurnLocation,
    pub pending: Option<(f64, Side, Option<Target>)>,
    pub reason: &'static str,
    cancelled_target: Option<(f64, Side, Option<Target>)>,
    pub journey: TurnJourney,
}

pub struct AutomaticInput<'a> {
    pub now: f64,
    pub mode: i32,
    pub valid: bool,
    pub cue: f64,
    pub direction: Option<Side>,
    pub atc_type: &'a str,
    pub desire: &'a str,
    pub lane: Option<Side>,
    pub speed: f64,
    pub angle: f64,
    pub heading: Option<f64>,
    pub distance: Option<f64>,
    pub position: Option<Position>,
    pub target: Option<Target>,
    pub travel: Option<(u64, f64)>,
    pub use_travel: bool,
    pub pressed: bool,
    pub torque: f64,
}

impl Default for AutomaticSignals {
    fn default() -> Self {
        Self::new()
    }
}

impl AutomaticSignals {
    pub fn new() -> Self {
        Self {
            cue: None,
            direction: None,
            turned: false,
            centered: None,
            mode: None,
            heading: None,
            rotation: 0.,
            location: TurnLocation::empty(),
            pending: None,
            reason: "idle",
            cancelled_target: None,
            journey: TurnJourney::new(),
        }
    }

    fn lane(valid: bool, lane: Option<Side>) -> Option<Automatic> {
        lane.filter(|_| valid).map(|side| ("lane".to_owned(), side))
    }

    pub fn update(&mut self, i: AutomaticInput<'_>) -> Option<Automatic> {
        if i.use_travel {
            let result = self.journey.update(JourneyInput {
                now: i.now,
                mode: i.mode,
                valid: i.valid,
                cue: i.cue,
                direction: i.direction,
                atc_type: i.atc_type,
                travel: i.travel,
                distance: i.distance,
                speed: i.speed,
                angle: i.angle,
                heading: i.heading,
                pressed: i.pressed,
                torque: i.torque,
                target: i.target.clone(),
                cancelled: false,
            });
            self.direction = self.journey.active.as_ref().map(|a| a.direction);
            self.reason = self.journey.reason;
            return result
                .map(|side| ("turn".to_owned(), side))
                .or_else(|| Self::lane(i.valid, i.lane));
        }
        let previous = self.cue.replace(i.cue);
        let changed_mode = self.mode != Some(i.mode);
        self.mode = Some(i.mode);
        if !i.valid || i.mode == 0 || changed_mode {
            self.direction = None;
            self.turned = false;
            self.centered = None;
            self.heading = None;
            self.rotation = 0.;
            self.pending = None;
            self.location = TurnLocation::empty();
            self.reason = "inputs";
            if !i.valid {
                self.cue = None;
            }
            return Self::lane(i.valid, i.lane);
        }
        // Consume cues even while disabled; never join a cue present at
        // startup, reconnect or setting change, nor replay a consumed one.
        let mut fresh_cue = previous.is_some_and(|p| p != i.cue)
            && i.cue.is_finite()
            && (0. ..0.35).contains(&(i.now - i.cue))
            && i.direction.is_some_and(|d| i.atc_type == d.turn());
        if fresh_cue {
            if let (Some(target), Some((at, side, Some(point)))) =
                (&i.target, &self.cancelled_target)
            {
                let (east, north) = offset((point.0, point.1), (target.0, target.1));
                if (0. ..=60.).contains(&(i.now - at))
                    && i.direction == Some(*side)
                    && target.2 == point.2
                    && east.hypot(north) <= 20.
                {
                    fresh_cue = false;
                    self.reason = "cancelled_target";
                }
            }
        }
        if fresh_cue {
            let direction = i.direction.expect("checked");
            match self.direction {
                None => {
                    self.direction = Some(direction);
                    self.turned = false;
                    self.centered = None;
                    self.heading = i.heading;
                    self.rotation = 0.;
                    self.location =
                        TurnLocation::new(i.position, i.target.clone(), i.heading, i.distance);
                    self.reason = "approach";
                }
                Some(current) if current != direction => {
                    // Keep the next cue briefly while finishing this corner.
                    self.pending = Some((i.cue, direction, i.target.clone()));
                }
                _ => {}
            }
        }
        if let (Some(target), Some(current)) = (&i.target, &self.location.target) {
            if target.2 != current.2 {
                self.direction = None;
                self.pending = None;
                self.reason = "navigation_session";
            }
        }
        if let Some((at, side, _)) = &self.pending {
            if i.now - at > 3. || i.atc_type != side.turn() {
                self.pending = None;
            }
        }
        if let Some(direction) = self.direction {
            let expected = match direction {
                Side::Left => "turnLeft",
                Side::Right => "turnRight",
            };
            match i.heading {
                Some(heading) if [i.speed, i.angle, heading].iter().all(|v| v.is_finite()) => {
                    if let Some(previous) = self.heading {
                        if i.speed >= 1. / 3.6 {
                            let delta = heading - previous;
                            self.rotation += delta.sin().atan2(delta.cos()) * -direction.sign();
                        }
                    }
                    self.heading = Some(heading);
                    let signed = i.angle * direction.sign();
                    self.turned |=
                        signed >= 30. && i.speed >= 1. / 3.6 && self.rotation >= 20f64.to_radians();
                    let (location_passed, straight_passed) = self.location.update(
                        i.position,
                        Some(heading),
                        i.speed,
                        i.angle,
                        self.turned,
                    );
                    let mut passed = i.distance.is_none_or(|d| {
                        d.is_finite() && (d <= 0. || i.atc_type != direction.turn())
                    });
                    if self.location.target.is_some() && i.position.is_some() {
                        passed = location_passed;
                    }
                    if straight_passed {
                        self.cancelled_target =
                            Some((i.now, direction, self.location.target.clone()));
                        self.direction = None;
                        self.pending = None;
                        self.reason = "straight_passed";
                    }
                    if self.turned && passed && i.angle.abs() <= 10. && i.speed >= 1. / 3.6 {
                        let since = *self.centered.get_or_insert(i.now);
                        if i.now - since >= 0.4 {
                            self.direction = None;
                            self.reason = "completed";
                        }
                    } else {
                        self.centered = None;
                    }
                    // Before entering, navigation cancellation still cancels.
                    if let Some(direction) = self.direction {
                        if (self.pending.is_none()
                            && matches!(i.desire, "turnLeft" | "turnRight")
                            && i.desire != expected)
                            || (!self.turned && i.atc_type != direction.turn())
                        {
                            self.direction = None;
                            self.pending = None;
                            self.reason = "navigation";
                        }
                    }
                }
                _ => {
                    self.direction = None;
                    self.pending = None;
                    self.reason = "heading";
                }
            }
            if let Some(direction) = self.direction {
                return Some(("turn".to_owned(), direction));
            }
        }
        if self.reason == "completed" {
            if let Some((_, direction, next_target)) = self.pending.take() {
                self.direction = Some(direction);
                self.turned = false;
                self.centered = None;
                self.heading = i.heading;
                self.rotation = 0.;
                self.location = TurnLocation::new(i.position, next_target, i.heading, i.distance);
                self.reason = "approach";
                return Some(("turn".to_owned(), direction));
            }
        }
        i.lane.map(|side| ("lane".to_owned(), side))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(now: f64, commands: &'a [Command], speed: f64, angle: f64) -> PolicyInput<'a> {
        PolicyInput {
            now,
            mode: 1,
            ok: true,
            commands,
            speed,
            angle,
            gear: "drive",
            remote: true,
            automatic: None,
            auto_valid: true,
        }
    }

    #[test]
    fn bluetooth_tap_holds_until_the_turn_is_completed_and_same_button_cancels() {
        let mut policy = Policy::new();
        let tap = |id: &str, t: f64, d: Side| Command {
            id: id.into(),
            time: t,
            direction: d,
            device: Some("aa".into()),
        };
        let commands = [tap("a", 1.0, Side::Left)];
        let (_, hold) = policy.update(input(1.0, &commands, 0., 0.));
        assert_eq!(hold.as_ref().map(|h| h.direction), Some(Side::Left));
        // Repeated delivery of the same event id is ignored.
        assert!(policy.update(input(1.1, &commands, 0., 0.)).1.is_some());
        // Turn into the corner, then return to centre for 0.4 s.
        policy.update(input(2.0, &[], 3., 40.));
        policy.update(input(3.0, &[], 3., 0.));
        assert!(policy.update(input(3.5, &[], 3., 0.)).1.is_none());

        let (_, hold) = policy.update(input(4.0, &[tap("b", 4.0, Side::Right)], 0., 0.));
        assert!(hold.is_some());
        assert!(policy
            .update(input(4.2, &[tap("c", 4.2, Side::Right)], 0., 0.))
            .1
            .is_none());
    }

    #[test]
    fn fast_taps_are_reported_without_holding() {
        let mut policy = Policy::new();
        let commands = [Command {
            id: "x".into(),
            time: 1.0,
            direction: Side::Right,
            device: None,
        }];
        let (taps, hold) = policy.update(input(1.0, &commands, 20., 0.));
        assert!(hold.is_none());
        assert_eq!(taps.len(), 1);
        assert!(policy.update(input(1.5, &[], 20., 0.)).0.is_empty());
    }
}
