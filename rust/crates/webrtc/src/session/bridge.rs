use crate::{cereal, Error};
use openpilot_messaging::services;
use openpilot_msgq::Publisher;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct Publishers {
    sockets: HashMap<String, Publisher>,
}

impl Publishers {
    pub(super) fn add(&mut self, names: &[String]) -> Result<(), Error> {
        for name in names {
            if !self.sockets.contains_key(name) {
                let capacity =
                    services::lookup(name).map_or(1024 * 1024, |service| service.queue_size);
                self.sockets
                    .insert(name.clone(), Publisher::for_runtime(name, capacity)?);
            }
        }
        Ok(())
    }

    pub(super) fn send(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        let nanos = u64::try_from(now.tv_nsec)?;
        let ns = u64::try_from(now.tv_sec)?
            .checked_mul(1_000_000_000)
            .and_then(|seconds| seconds.checked_add(nanos))
            .ok_or(Error::Contract("monotonic timestamp range"))?;
        let (name, bytes) = cereal::incoming(std::str::from_utf8(bytes)?, ns)?;
        self.sockets
            .get_mut(&name)
            .ok_or_else(|| Error::Service(name))?
            .send(&bytes)?;
        Ok(())
    }
}
