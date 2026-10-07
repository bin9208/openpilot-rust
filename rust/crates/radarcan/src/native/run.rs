use super::{io::NativeIo, platform, Options};
use crate::{databases::Databases, numerics::Numerics, runtime::Engine, wire, Error};
use rustix::time::ClockId;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub fn run(options: Options) -> Result<(), Error> {
    let stop = Arc::new(AtomicUsize::new(0));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register_usize(
            signal,
            Arc::clone(&stop),
            usize::try_from(signal).map_err(|_| Error::IntegerOverflow)?,
        )?;
    }
    platform::configure()?;
    let mut io = NativeIo::new(
        Databases::new(options.dbc),
        Numerics::load(&options.numerics)?,
    )?;
    io.fixture = options.fixture.map(|paths| super::fixture::Fixture {
        paths,
        stop: Arc::clone(&stop),
    });
    let config = loop {
        if stop.load(Ordering::Relaxed) != 0 {
            return Err(Error::Signal(signal_hook::consts::SIGINT));
        }
        if let Some(bytes) = io.car_params()? {
            break wire::config(&bytes)?;
        }
        thread::sleep(Duration::from_millis(100));
    };
    let mut engine = Engine::new(config, &mut io, std::env::var_os("REPLAY").is_some())?;
    let mut remaining = options.steps.map(std::num::NonZeroU64::get);
    while stop.load(Ordering::Relaxed) == 0 {
        io.poll()?;
        if stop.load(Ordering::Relaxed) != 0 {
            break;
        }
        let start = platform::seconds(ClockId::Monotonic);
        let cpu_start = platform::seconds(ClockId::ThreadCPUTime);
        let packets = io
            .drain(0)?
            .into_iter()
            .map(|bytes| wire::can(&bytes))
            .collect::<Result<Vec<_>, _>>()?;
        engine.batches.add_can(packets);
        for bytes in io.drain(1)? {
            engine.add_state(wire::ego(&bytes)?);
        }
        let decode_done = platform::seconds(ClockId::Monotonic);
        let metrics = engine.process(&mut io)?;
        let work = (platform::seconds(ClockId::Monotonic) - start) * 1000.;
        let cpu = (platform::seconds(ClockId::ThreadCPUTime) - cpu_start) * 1000.;
        let decode = (decode_done - start) * 1000.;
        let radar = (platform::seconds(ClockId::Monotonic) - decode_done) * 1000.;
        io.record(metrics, [work, cpu, decode, radar])?;
        if let Some(count) = remaining.as_mut() {
            *count -= 1;
        }
        if remaining == Some(0) {
            break;
        }
    }
    let signal = stop.load(Ordering::Relaxed);
    if signal != 0 {
        return Err(Error::Signal(
            i32::try_from(signal).map_err(|_| Error::IntegerOverflow)?,
        ));
    }
    Ok(())
}
