//! Command selection and packet authentication of carrot's
//! `bluetooth/b1_monitor.py` (`Intent`, `command`).
use hmac::{Hmac, Mac};
use openpilot_blink::{policy::Command, source::Record, Side};
use sha2::Sha256;
use std::collections::VecDeque;

pub const STATES: [&str; 6] = [
    "missing",
    "ready",
    "sending",
    "waiting",
    "driver_override",
    "error",
];

/// `command(key, nonce, stamp, seq, op, ttl)`: 12-byte body + 8-byte HMAC.
pub fn command(key: &[u8], nonce: u32, stamp: u32, seq: u16, op: u8, ttl: f64) -> Vec<u8> {
    let lease = (ttl * 200.).clamp(1., 70.) as u8;
    let mut body = Vec::with_capacity(20);
    body.extend_from_slice(&nonce.to_le_bytes());
    body.extend_from_slice(&stamp.to_le_bytes());
    body.extend_from_slice(&seq.to_le_bytes());
    body.push(op);
    body.push(lease);
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else {
        return body;
    };
    mac.update(&body);
    let digest = mac.finalize().into_bytes();
    body.extend_from_slice(&digest[..8]);
    body
}

/// The board's 15-byte report (`<IIHBBBH`).
#[derive(Clone, Copy, Debug)]
pub struct Report {
    pub nonce: u32,
    pub stamp: u32,
    pub ack: u16,
    pub state: u8,
    pub lamps: u8,
    pub version: u8,
    pub last_seq: u16,
}

impl Report {
    pub fn parse(raw: &[u8]) -> Option<Self> {
        if raw.len() != 15 {
            return None;
        }
        let u32_at = |i: usize| u32::from_le_bytes([raw[i], raw[i + 1], raw[i + 2], raw[i + 3]]);
        let u16_at = |i: usize| u16::from_le_bytes([raw[i], raw[i + 1]]);
        Some(Self {
            nonce: u32_at(0),
            stamp: u32_at(4),
            ack: u16_at(8),
            state: raw[10],
            lamps: raw[11],
            version: raw[12],
            last_seq: u16_at(13),
        })
    }
}

#[derive(Clone, Debug)]
struct Queued {
    event: Command,
    held: bool,
    until: f64,
}

pub struct Intent {
    started: f64,
    seen: VecDeque<String>,
    /// `(seq, id, sent at)`.
    pub inflight: Option<(u16, String, f64)>,
    queued: Option<Queued>,
    pub seq: u16,
}

impl Intent {
    pub fn new(now: f64) -> Self {
        Self {
            started: now,
            seen: VecDeque::with_capacity(256),
            inflight: None,
            queued: None,
            seq: 0,
        }
    }

    pub fn queued(&self) -> bool {
        self.queued.is_some()
    }

    pub fn clear(&mut self) {
        self.inflight = None;
        self.queued = None;
    }

    /// `(op, request id, ttl)`: 1 left tap, 2 right tap, 3 cancel, 4/5 hold.
    pub fn choose(
        &mut self,
        source: Option<&Record>,
        state: &str,
        ack: u16,
        now: f64,
    ) -> (u8, Option<String>, f64) {
        let Some(source) = source.filter(|s| {
            s.ok && (0. ..0.20).contains(&(now - s.stamp))
                && !matches!(state, "error" | "driver_override" | "missing")
        }) else {
            self.clear();
            return (3, None, 0.35);
        };
        let guard = source.stamp + 0.35 - now;
        let hold = source.hold.as_ref();
        let mut candidates: Vec<(Command, bool)> =
            source.events.iter().map(|e| (e.clone(), false)).collect();
        if let Some(h) = hold {
            candidates.push((
                Command {
                    id: h.id.clone(),
                    time: h.time,
                    direction: h.direction,
                    device: h.device.clone(),
                },
                true,
            ));
        }
        for (event, held) in candidates {
            if self.seen.contains(&event.id)
                || !(self.started <= event.time && event.time <= now)
                || now - event.time >= 0.4
            {
                continue;
            }
            if self.seen.len() == 256 {
                self.seen.pop_front();
            }
            self.seen.push_back(event.id.clone());
            if state == "ready"
                || (self.inflight.is_some() && matches!(state, "sending" | "waiting"))
            {
                let lifetime = if self.inflight.is_some() && state != "ready" {
                    3.2
                } else {
                    0.4
                };
                let until = event.time + lifetime;
                self.queued = Some(Queued { event, held, until });
            }
        }
        if let Some(q) = self.queued.clone() {
            let device_gone = q
                .event
                .device
                .as_ref()
                .is_some_and(|d| !source.devices.contains(d));
            if now >= q.until || (q.held && hold.is_none_or(|h| h.id != q.event.id)) || device_gone
            {
                self.queued = None;
            } else if state == "ready" && self.inflight.as_ref().is_none_or(|i| ack == i.0) {
                self.queued = None;
                let op = if q.event.direction == Side::Left {
                    1
                } else {
                    2
                };
                return (op, Some(q.event.id), guard.min(q.until - now));
            }
        }
        if source.b1_hold {
            if let (Some(h), Some(i)) = (hold, &self.inflight) {
                if ack == i.0 && h.id == i.1 && now - h.time < 300. {
                    return (if h.direction == Side::Left { 4 } else { 5 }, None, guard);
                }
            }
        }
        (0, None, guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_packs_lease_and_truncated_hmac() {
        let packet = command(&[7; 32], 1, 2, 3, 1, 0.35);
        assert_eq!(packet.len(), 20);
        assert_eq!(&packet[..12], &[1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 1, 70]);
        assert_eq!(command(&[7; 32], 1, 2, 3, 1, 0.001)[11], 1);
    }

    #[test]
    fn a_fresh_tap_is_sent_once_when_ready() {
        let mut intent = Intent::new(0.);
        let record = Record {
            stamp: 10.,
            ok: true,
            events: vec![Command {
                id: "a".into(),
                time: 10.,
                direction: Side::Right,
                device: None,
            }],
            ..Record::default()
        };
        let (op, id, ttl) = intent.choose(Some(&record), "ready", 0, 10.05);
        assert_eq!((op, id.as_deref()), (2, Some("a")));
        assert!(ttl > 0.);
        assert_eq!(intent.choose(Some(&record), "ready", 0, 10.06).0, 0);
        assert_eq!(intent.choose(None, "ready", 0, 10.07).0, 3);
    }
}
