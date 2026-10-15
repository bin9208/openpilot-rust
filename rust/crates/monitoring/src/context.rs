//! Non-actuating context and physical interaction filters from final dm2_context.py.
use std::collections::{HashSet, VecDeque};
#[derive(Clone, Copy, Debug)]
pub struct ObjectObservation {
    pub x: f64,
    pub y: f64,
    pub speed: f64,
    pub relative_speed: f64,
}
#[derive(Clone, Copy)]
struct TrafficTrack {
    last_seen: f64,
    first_seen: f64,
    observation: ObjectObservation,
    confirmed: bool,
}
#[derive(Default)]
pub struct TrafficContext {
    tracks: Vec<TrafficTrack>,
    pub strict_until: f64,
    clear_since: Option<f64>,
    moving_present: bool,
}
impl TrafficContext {
    pub fn update(
        &mut self,
        now: f64,
        objects: &[ObjectObservation],
        healthy: bool,
        straight: bool,
        coverage: bool,
    ) -> (bool, bool) {
        if !healthy {
            self.clear_since = None;
            return (true, false);
        }
        let old: Vec<_> = self
            .tracks
            .iter()
            .copied()
            .filter(|t| now - t.last_seen <= 2.)
            .collect();
        let mut remaining: HashSet<usize> = (0..old.len()).collect();
        let mut current: Vec<TrafficTrack> = Vec::new();
        let mut new_moving = false;
        for &obj in objects {
            if ![obj.x, obj.y, obj.speed, obj.relative_speed]
                .iter()
                .all(|v| v.is_finite())
            {
                self.clear_since = None;
                return (true, false);
            }
            if !(-10. ..=150.).contains(&obj.x) || obj.y.abs() > 6. || obj.speed.abs() < 2. {
                continue;
            }
            if current.iter().any(|t| {
                (obj.x - t.observation.x).abs() < 2. && (obj.y - t.observation.y).abs() < 1.
            }) {
                continue;
            }
            let candidate = remaining
                .iter()
                .copied()
                .filter_map(|i| {
                    let t = old[i];
                    let dx = (obj.x
                        - (t.observation.x + t.observation.relative_speed * (now - t.last_seen)))
                        .abs();
                    let dy = (obj.y - t.observation.y).abs();
                    (dx < 8. && dy < 2.).then_some((dx + 3. * dy, i))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let (mut first, mut confirmed) = (now, false);
            if let Some((_, i)) = candidate {
                remaining.remove(&i);
                let previous = old[i];
                first = if now - previous.last_seen <= 0.3 {
                    previous.first_seen
                } else {
                    now
                };
                confirmed = previous.confirmed;
            }
            if !confirmed && now - first >= 0.2 {
                confirmed = true;
                new_moving = true;
            }
            current.push(TrafficTrack {
                last_seen: now,
                first_seen: first,
                observation: obj,
                confirmed,
            });
        }
        let mut rest: Vec<_> = remaining.into_iter().collect();
        rest.sort_unstable();
        current.extend(rest.into_iter().map(|i| old[i]));
        self.tracks = current;
        if self.tracks.is_empty() {
            self.moving_present = false;
        } else if new_moving && !self.moving_present {
            self.strict_until = now + 20.;
            self.moving_present = true;
        }
        let clear = coverage && straight && self.tracks.is_empty();
        if clear {
            self.clear_since.get_or_insert(now);
        } else {
            self.clear_since = None;
        }
        (
            now < self.strict_until,
            self.clear_since.is_some_and(|t| now - t >= 10.),
        )
    }
}
#[derive(Default)]
pub struct CancelPressSequence {
    count: u32,
    first: Option<f64>,
    held: bool,
}
impl CancelPressSequence {
    pub fn reset_input_stream(&mut self) {
        *self = Self::default();
    }
    pub fn update(&mut self, now: f64, buttons: &[(String, bool)]) -> bool {
        if !now.is_finite() {
            self.count = 0;
            self.first = None;
        }
        let mut triggered = false;
        for (kind, pressed) in buttons {
            if kind != "cancel" {
                self.count = 0;
                self.first = None;
                triggered = false;
                continue;
            }
            if !pressed {
                self.held = false;
                continue;
            }
            if self.held {
                continue;
            }
            self.held = true;
            if !now.is_finite() {
                continue;
            }
            if self.count == 0
                || self
                    .first
                    .is_none_or(|first| !(0. ..=3.).contains(&(now - first)))
            {
                self.count = 1;
                self.first = Some(now);
            } else {
                self.count += 1;
            }
            if self.count == 3 {
                triggered = true;
                self.count = 0;
                self.first = None;
            }
        }
        triggered
    }
}
#[derive(Default)]
pub struct AutomaticCancelFilter {
    requests: VecDeque<f64>,
    suppress_release: bool,
    suppress_until: f64,
}
impl AutomaticCancelFilter {
    pub fn record(&mut self, now: f64, requested: bool) {
        if requested && now.is_finite() {
            if self.requests.len() == 64 {
                self.requests.pop_front();
            }
            self.requests.push_back(now);
        }
    }
    pub fn reset_input_stream(&mut self) {
        self.suppress_release = false;
        self.suppress_until = f64::NEG_INFINITY;
    }
    pub fn filter_buttons(
        &mut self,
        now: f64,
        buttons: Vec<(String, bool)>,
    ) -> Vec<(String, bool)> {
        if now.is_finite() && now > self.suppress_until {
            self.suppress_release = false;
        }
        buttons
            .into_iter()
            .filter(|(kind, pressed)| {
                if kind != "cancel" {
                    return true;
                }
                if *pressed {
                    while self.requests.front().is_some_and(|t| now - t > 0.15) {
                        self.requests.pop_front();
                    }
                    let physical = now.is_finite()
                        && !self
                            .requests
                            .iter()
                            .any(|t| (0. ..=0.15).contains(&(now - t)));
                    if !physical {
                        self.suppress_release = true;
                        self.suppress_until = now + 0.5;
                    }
                    physical
                } else if self.suppress_release {
                    self.suppress_release = false;
                    self.suppress_until = f64::NEG_INFINITY;
                    false
                } else {
                    true
                }
            })
            .collect()
    }
}
#[derive(Default)]
pub struct InteractionEdges {
    previous: [bool; 3],
    held: HashSet<String>,
    pub last_response: Option<f64>,
}
impl InteractionEdges {
    pub fn speed_button(kind: &str) -> bool {
        matches!(
            kind,
            "accelCruise" | "decelCruise" | "resumeCruise" | "setCruise"
        )
    }
    pub fn update(
        &mut self,
        now: f64,
        gas: bool,
        brake: bool,
        steering: bool,
        buttons: &[(String, bool)],
    ) -> bool {
        let state = [gas, brake, steering];
        let mut response = state.iter().zip(self.previous).any(|(v, p)| *v && !p);
        self.previous = state;
        for (kind, pressed) in buttons {
            if !(Self::speed_button(kind)
                || matches!(
                    kind.as_str(),
                    "gapAdjustCruise" | "cancel" | "mainCruise" | "lkas" | "lfaButton"
                ))
            {
                continue;
            }
            if *pressed {
                response |= self.held.insert(kind.clone());
            } else {
                self.held.remove(kind);
            }
        }
        if response {
            self.last_response = Some(now);
        }
        response
    }
}
#[derive(Default)]
pub struct SteeringTouchEvidence {
    previous: Option<bool>,
    last_timestamp: u64,
}
impl SteeringTouchEvidence {
    pub fn update(
        &mut self,
        now: f64,
        available: bool,
        valid: bool,
        timestamp: u64,
        touched: bool,
        car_valid: bool,
    ) -> (bool, bool) {
        let fresh = car_valid
            && available
            && valid
            && timestamp > 0
            && (0. ..=0.25).contains(&(now - timestamp as f64 / 1e9))
            && timestamp >= self.last_timestamp;
        if !fresh {
            self.previous = None;
            return (false, false);
        }
        let edge = touched && self.previous == Some(false) && timestamp > self.last_timestamp;
        self.previous = Some(touched);
        self.last_timestamp = timestamp;
        (touched, edge)
    }
}
#[derive(Default)]
pub struct CameraAvailability {
    healthy_since: Option<f64>,
}
impl CameraAvailability {
    pub fn update(&mut self, now: f64, usable: bool) -> bool {
        if !usable {
            self.healthy_since = None;
            return false;
        }
        now - *self.healthy_since.get_or_insert(now) >= 2.
    }
}
