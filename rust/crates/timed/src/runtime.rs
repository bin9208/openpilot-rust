use crate::{
    clock::{self, Clock},
    timezone::{self, Internet, Services},
    wire, Error,
};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::Options,
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Default)]
pub struct State {
    pub last_timezone_attempt: u64,
}
impl State {
    /// Called after clocks publication, preserving the original side-effect order.
    pub fn step(
        &mut self,
        gps: wire::Gps,
        host: &mut Host<'_>,
        stop: &AtomicBool,
    ) -> Result<bool, Error> {
        let usable = gps.usable(host.clock.monotonic()?);
        let priority = timezone::priority(&timezone::current(&mut host.services)?);
        if priority < timezone::priority("wifi") {
            let interval = if priority == 0 {
                30_000_000_000
            } else {
                300_000_000_000
            };
            let now = host.clock.monotonic()?;
            if now.saturating_sub(self.last_timezone_attempt) > interval {
                self.last_timezone_attempt = host.clock.monotonic()?;
                let zone = host
                    .internet
                    .lookup_until_stopped(host.services.paths, stop);
                if stop.load(Ordering::Relaxed) {
                    return Ok(false);
                }
                if let Some(zone) = zone {
                    timezone::apply(&zone, "wifi", &mut host.services)?;
                } else if usable {
                    timezone::apply(
                        &timezone::from_gps(gps.longitude)?,
                        "gps",
                        &mut host.services,
                    )?;
                }
            }
        }
        if !usable {
            return Ok(false);
        }
        let epoch = gps.epoch();
        let local = host.clock.local(epoch)?;
        let (minimum, maximum) = clock::bounds(host.clock, &host.services.paths.systemd)?;
        if local < minimum || local > maximum {
            return Ok(false);
        }
        crate::set_time(epoch, host.clock, &mut host.services)?;
        host.clock.sleep(Duration::from_secs(10), stop);
        Ok(true)
    }
}
pub struct Host<'a> {
    pub clock: &'a dyn Clock,
    pub services: Services<'a>,
    pub internet: &'a Internet,
}
pub fn run(host: &mut Host<'_>, cycles: Option<u64>, stop: &AtomicBool) -> Result<(), Error> {
    let service = if host.services.params.get_bool("UbloxAvailable")? {
        "gpsLocationExternal"
    } else {
        "gpsLocation"
    };
    let mut publisher = PubMaster::for_runtime(&["clocks"])?;
    let mut subscriber = SubMaster::for_runtime(&[service], Options::default())?;
    let mut state = State::default();
    let mut remaining = cycles;
    while !stop.load(Ordering::Relaxed) {
        subscriber.update(Duration::from_secs(1))?;
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let monotonic = host.clock.monotonic()?;
        let valid = clock::valid(host.clock, &host.services.paths.systemd)?;
        let wall = host.clock.wall_nanos()?;
        publisher.send("clocks", &wire::clocks(wall, monotonic, valid))?;
        state.step(wire::gps(&subscriber.state, service)?, host, stop)?;
        if let Some(count) = &mut remaining {
            *count -= 1;
            if *count == 0 {
                break;
            }
        }
    }
    Ok(())
}
