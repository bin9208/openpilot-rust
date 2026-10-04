use super::{Io, Metrics, Reason};
use crate::{
    batch::{Batches, Ego, MAX_AGE_NS},
    data::Data,
    decoder::{Config, Interface},
    scalar, Error,
};

pub struct Engine {
    pub batches: Batches,
    pub radar: Interface,
    config: Config,
    pub radar_track_flip: bool,
    pub last_input_ns: u64,
    pub last_can_input_ns: u64,
    pub last_error_publish_ns: u64,
    pub needs_reset: bool,
    pub replay: bool,
    pub replay_ns: u64,
}

impl Engine {
    pub fn new(config: Config, io: &mut impl Io, replay: bool) -> Result<Self, Error> {
        let radar = io.create_interface(&config)?;
        let radar_track_flip = io.track_flip()?;
        let started = io.monotonic_ns();
        Ok(Self {
            batches: Batches::default(),
            radar,
            config,
            radar_track_flip,
            last_input_ns: started,
            last_can_input_ns: started,
            last_error_publish_ns: 0,
            needs_reset: false,
            replay,
            replay_ns: started,
        })
    }

    pub fn add_state(&mut self, ego: Ego) {
        self.batches.add_state(ego);
        if self.replay {
            self.replay_ns = ego.receive_ns;
        }
    }

    fn now(&self, io: &mut impl Io) -> u64 {
        if self.replay {
            self.replay_ns
        } else {
            io.monotonic_ns()
        }
    }

    fn publish_error(&mut self, reason: Reason, now: u64, io: &mut impl Io) -> Result<(), Error> {
        if !self.needs_reset {
            io.input_error(reason)?;
        }
        self.needs_reset = true;
        if elapsed(now, self.last_error_publish_ns) >= 50_000_000 {
            let mut data = Data::default();
            data.errors.can_error = true;
            data.radar_track_flipped = self.radar_track_flip;
            io.publish(data, false)?;
            self.last_error_publish_ns = now;
        }
        Ok(())
    }

    pub fn process(&mut self, io: &mut impl Io) -> Result<Metrics, Error> {
        let mut metrics = Metrics::default();
        loop {
            let now = self.now(io);
            let Some(batch) = self.batches.take(now) else {
                break;
            };
            let now = self.now(io);
            if let Some(reason) = batch.error {
                self.publish_error(Reason::Batch(reason), now, io)?;
                continue;
            }
            if !batch.packets.is_empty() {
                self.last_can_input_ns = batch.ego.receive_ns;
            } else if elapsed(now, self.last_can_input_ns) > i128::from(MAX_AGE_NS) {
                self.publish_error(Reason::CanTimeout, now, io)?;
                continue;
            }
            if self.needs_reset {
                self.radar = io.create_interface(&self.config)?;
                self.needs_reset = false;
            }
            self.last_input_ns = batch.ego.receive_ns;
            metrics.input_age_ms = scalar::maximum(
                metrics.input_age_ms,
                elapsed(now, batch.ego.receive_ns) as f64 / 1e6,
            );
            let result = io.update(&mut self.radar, batch.ego, &batch.packets)?;
            metrics.processed_batches += 1;
            if elapsed(self.now(io), batch.ego.receive_ns) > i128::from(MAX_AGE_NS) {
                let now = self.now(io);
                self.publish_error(Reason::ProcessingTimeout, now, io)?;
                continue;
            }
            if let Some(mut result) = result {
                let valid = !result.errors.any();
                result.set_flip(self.radar_track_flip);
                io.publish(result, valid)?;
            }
        }
        let now = self.now(io);
        if elapsed(now, self.last_input_ns.min(self.last_can_input_ns)) > i128::from(MAX_AGE_NS) {
            self.publish_error(Reason::InputTimeout, now, io)?;
        }
        metrics.invalid = self.needs_reset;
        metrics.pending_states = self.batches.states.len();
        metrics.pending_can = self.batches.can.len();
        Ok(metrics)
    }
}

fn elapsed(now: u64, then: u64) -> i128 {
    i128::from(now) - i128::from(then)
}
