use crate::{peer::Peer, Error};
use bytes::BytesMut;
use num_traits::ToPrimitive;
use openpilot_messaging::{runtime::SubMaster, state::Options};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Cadence {
    last_send: HashMap<&'static str, f64>,
    sequence: HashMap<&'static str, u16>,
}

fn interval(service: &str) -> f64 {
    match service {
        "carState" | "controlsState" | "carControl" => 1.0 / 30.0,
        "roadCameraState"
        | "liveCalibration"
        | "liveParameters"
        | "liveTorqueParameters"
        | "liveDelay" => 0.25,
        "deviceState" | "peripheralState" | "gpsLocationExternal" => 0.5,
        "selfdriveState" => 0.2,
        _ => 0.0,
    }
}

impl Cadence {
    fn prepare(&mut self, service: &'static str, now: f64, buffered: usize) -> Option<u16> {
        let interval = interval(service);
        if interval > 0.0 && now - self.last_send.get(service).copied().unwrap_or(0.0) < interval {
            return None;
        }
        self.last_send.insert(service, now);
        if buffered >= 16 * 1024 {
            return None;
        }
        let sequence = self.sequence.entry(service).or_default();
        *sequence = sequence.wrapping_add(1);
        Some(*sequence)
    }
}

pub(super) struct Compact {
    subscriptions: SubMaster,
    cadence: Cadence,
    last_update: Instant,
}

fn monotonic_seconds() -> Result<f64, Error> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let ns = u64::try_from(now.tv_sec)?
        .checked_mul(1_000_000_000)
        .and_then(|seconds| {
            u64::try_from(now.tv_nsec)
                .ok()
                .and_then(|nanos| seconds.checked_add(nanos))
        })
        .ok_or(Error::Contract("compact monotonic timestamp range"))?;
    Ok(ns
        .to_f64()
        .ok_or(Error::Contract("compact monotonic float range"))?
        / 1_000_000_000.0)
}

impl Compact {
    pub(super) fn new() -> Result<Self, Error> {
        Ok(Self {
            subscriptions: SubMaster::for_runtime(
                &openpilot_carrot_state::services().collect::<Vec<_>>(),
                Options::default(),
            )?,
            cadence: Cadence::default(),
            last_update: Instant::now(),
        })
    }

    pub(super) fn update(&mut self, peer: &mut Peer) -> Result<(), Error> {
        if self.last_update.elapsed() < Duration::from_millis(10) {
            return Ok(());
        }
        self.last_update = Instant::now();
        self.subscriptions.update(Duration::ZERO)?;
        if !peer.channel.open(&mut peer.rtc) {
            return Ok(());
        }
        let now = monotonic_seconds()?;
        for service in openpilot_carrot_state::services() {
            let topic = self
                .subscriptions
                .state
                .topics()
                .iter()
                .find(|topic| topic.service.name == service)
                .ok_or(Error::Contract("compact subscription missing"))?;
            if !topic.updated {
                continue;
            }
            let Some(sequence) = self.cadence.prepare(service, now, peer.channel.buffered) else {
                continue;
            };
            let event = topic.event()?;
            let capnp::dynamic_value::Reader::Struct(event) = event.into() else {
                return Err(Error::Contract("compact Event must be struct"));
            };
            let value = event.get_named(service)?;
            match openpilot_carrot_state::encode_reader(service, value, sequence) {
                Ok(frame) => {
                    if let Err(error) =
                        peer.channel
                            .enqueue(&mut peer.rtc, BytesMut::from(frame.as_slice()), false)
                    {
                        eprintln!("WebRTC compact state send failed for {service}: {error}");
                    }
                }
                Err(error) => eprintln!("WebRTC compact state send failed for {service}: {error}"),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Cadence;

    #[test]
    fn buffer_drop_advances_throttle_without_advancing_sequence() {
        let mut cadence = Cadence::default();
        assert_eq!(cadence.prepare("carState", 10.0, 16384), None);
        assert_eq!(cadence.prepare("carState", 10.02, 0), None);
        assert_eq!(cadence.prepare("carState", 10.04, 16383), Some(1));
        assert_eq!(cadence.prepare("carState", 10.08, 16384), None);
        assert_eq!(cadence.prepare("carState", 10.09, 0), None);
        assert_eq!(cadence.prepare("carState", 10.12, 0), Some(2));
    }

    #[test]
    fn unspecified_service_has_no_throttle_and_sequence_wraps() {
        let mut cadence = Cadence::default();
        cadence.sequence.insert("carrotMan", u16::MAX);
        assert_eq!(cadence.prepare("carrotMan", 10.0, 16383), Some(0));
        assert_eq!(cadence.prepare("carrotMan", 10.0, 0), Some(1));
    }
}
