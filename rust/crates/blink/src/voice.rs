//! Binds a voice maneuver to one existing physical-indicator hold. Port of
//! carrot's `tools/jetlink/blink_voice.py`.
use super::{observed::Meta, policy::Hold, source::Feedback, Side};
use openpilot_desire::types::Car;
use std::collections::VecDeque;

/// carrot `assistant_lane.IDENTITY`: `^[A-Za-z0-9_-]{43}$`.
pub fn identity(request: &str) -> bool {
    request.len() == 43
        && request
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// `secrets.token_urlsafe(32)`: 32 random bytes, unpadded URL-safe base64.
fn token() -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut bytes = [0u8; 33];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = std::io::Read::read_exact(&mut file, &mut bytes[..32]);
    }
    let mut text = String::with_capacity(44);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for shift in [18, 12, 6, 0] {
            text.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
        }
    }
    text.truncate(43);
    text
}

#[derive(Debug)]
pub struct VoiceLaneSignal {
    seen: VecDeque<String>,
    pub route: String,
    advertised_at: f64,
    saw_idle: bool,
    pub request: String,
    pub direction: Option<Side>,
    pub hold_id: Option<String>,
}

impl Default for VoiceLaneSignal {
    fn default() -> Self {
        Self {
            seen: VecDeque::with_capacity(128),
            route: token(),
            advertised_at: -1.,
            saw_idle: false,
            request: String::new(),
            direction: None,
            hold_id: None,
        }
    }
}

impl VoiceLaneSignal {
    pub fn advertise(&mut self, now: f64, available: bool, save: &mut dyn FnMut(&str, f64)) {
        if available && now - self.advertised_at >= 0.15 {
            save(&self.route, now);
            self.advertised_at = now;
        }
    }

    pub fn select(
        &mut self,
        meta: Option<&Meta>,
        car: &Car,
        allowed: bool,
        peer_ready: bool,
        hold: Option<&Hold>,
        busy: bool,
    ) -> Option<(String, Side)> {
        let request = meta
            .map_or("", |m| m.assistant_signal_id.as_str())
            .to_owned();
        let direction = Side::parse(meta.map_or("none", |m| m.lane_change_direction));
        let phase = meta.map_or("off", |m| m.lane_change_state);
        if request.is_empty() && meta.is_some() {
            self.saw_idle = true;
        }
        let allowed = allowed && meta.is_some_and(|m| m.assistant_signal_route == self.route);
        if request != self.request {
            self.request.clone_from(&request);
            self.hold_id = None;
            self.direction = None;
            let fresh = self.saw_idle && !self.seen.contains(&request) && identity(&request);
            if !request.is_empty() {
                if self.seen.len() == 128 {
                    self.seen.pop_front();
                }
                self.seen.push_back(request.clone());
            }
            if fresh
                && allowed
                && peer_ready
                && !busy
                && hold.is_none()
                && matches!(phase, "preLaneChange" | "laneChangeStarting")
                && direction.is_some()
                && !(car.left_blinker || car.right_blinker)
            {
                self.direction = direction;
            }
        }
        let opposite = match direction {
            Some(Side::Left) => car.right_blinker,
            _ => car.left_blinker,
        };
        if !allowed
            || busy
            || direction != self.direction
            || !matches!(
                phase,
                "preLaneChange" | "laneChangeStarting" | "laneChangeFinishing"
            )
            || hold.is_some_and(|h| Some(&h.id) != self.hold_id.as_ref())
            || (self.hold_id.is_some() && hold.is_none())
            || opposite
        {
            self.direction = None;
        }
        self.direction.map(|d| ("lane".to_owned(), d))
    }

    pub fn bind(&mut self, hold: Option<&Hold>) -> bool {
        if self.direction.is_some() && self.hold_id.is_none() {
            if let Some(h) =
                hold.filter(|h| h.source == "lane" && Some(h.direction) == self.direction)
            {
                self.hold_id = Some(h.id.clone());
                return true;
            }
            self.direction = None;
        }
        false
    }

    pub fn confirmed(
        &self,
        hold: Option<&Hold>,
        activated: Option<&str>,
        feedback: &Feedback,
        car: &Car,
    ) -> bool {
        let Some(direction) = self.direction else {
            return false;
        };
        // B1 boards sample the physical lamps even without lamp carState.
        let lit = match feedback.lamps {
            Some(lamps) => {
                lamps == if direction == Side::Left { 1 } else { 2 }
                    && !(if direction == Side::Left {
                        car.right_blinker
                    } else {
                        car.left_blinker
                    })
            }
            None => {
                (car.left_blinker, car.right_blinker)
                    == (direction == Side::Left, direction == Side::Right)
            }
        };
        hold.is_some_and(|h| {
            h.source == "lane"
                && h.direction == direction
                && self.hold_id.as_deref() == Some(&h.id)
                && activated == Some(h.id.as_str())
                && feedback.accepted_id.as_deref() == Some(&h.id)
        }) && lit
    }
}
