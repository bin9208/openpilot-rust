//! Port of openpilot/tools/joystick/joystick_control.py under the original MIT license.
mod device;

use crate::Error;
use capnp::{message::Builder, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::runtime::PubMaster;
use openpilot_params::Params;
use std::{
    num::NonZeroU64,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    KeyboardCli,
    GamepadCli,
    Managed,
}

pub struct Options {
    pub entry: Entry,
    pub input: Option<PathBuf>,
    pub frames: Option<NonZeroU64>,
}

pub fn encode(axes: [f64; 2], timestamp: u64) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_valid(true);
    root.set_log_mono_time(timestamp);
    let mut values = root.init_test_joystick().init_axes(2);
    for (index, value) in (0_u32..2).zip(axes) {
        values.set(index, value as f32);
    }
    serialize::write_message_to_words(&message)
}

pub fn run(options: Options) -> Result<(), Error> {
    let params = Params::for_runtime()?;
    match options.entry {
        Entry::KeyboardCli | Entry::GamepadCli => {
            let offroad = match params.get_bool("IsOffroad") {
                Ok(value) => value,
                Err(openpilot_params::Error::Io(_)) => false,
                Err(error) => return Err(error.into()),
            };
            if !offroad && std::env::var_os("ZMQ").is_none() {
                println!("The car must be off before running joystick_control.");
                return Ok(());
            }
        }
        Entry::Managed => (),
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let mut device = match options.entry {
        Entry::KeyboardCli => device::Device::keyboard()?,
        Entry::GamepadCli | Entry::Managed => device::Device::gamepad(options.input)?,
    };
    let names = device.names();
    match params.put_bool("JoystickDebugMode", true) {
        Ok(()) | Err(openpilot_params::Error::Io(_)) => (),
        Err(error) => return Err(error.into()),
    }
    let axes = Arc::new(Mutex::new([0.0; 2]));
    let input_axes = Arc::clone(&axes);
    let input_stop = Arc::clone(&stop);
    let worker = thread::spawn(move || -> Result<(), Error> {
        while !input_stop.load(Ordering::Relaxed) {
            match device.update()? {
                Some(value) => {
                    *input_axes
                        .lock()
                        .map_err(|_| Error::Contract("joystick state lock poisoned"))? = value
                }
                None => thread::sleep(Duration::from_millis(1)),
            }
        }
        Ok(())
    });
    let result = (|| {
        let mut publisher = PubMaster::for_runtime(&["testJoystick"])?;
        let mut frame = 0_u64;
        let mut deadline = None;
        while !stop.load(Ordering::Relaxed) && !worker.is_finished() {
            let values = *axes
                .lock()
                .map_err(|_| Error::Contract("joystick state lock poisoned"))?;
            if frame.is_multiple_of(20) {
                let numbers = values.map(|value| {
                    let text = format!("{value:.3}");
                    let trimmed = text.trim_end_matches('0');
                    if trimmed.ends_with('.') {
                        format!("{trimmed}0")
                    } else {
                        trimmed.to_owned()
                    }
                });
                println!(
                    "\n{}: {}, {}: {}",
                    names[0], numbers[0], names[1], numbers[1]
                );
            }
            let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
            let timestamp = u64::try_from(now.tv_sec)
                .ok()
                .and_then(|seconds| seconds.checked_mul(1_000_000_000))
                .and_then(|seconds| {
                    u64::try_from(now.tv_nsec)
                        .ok()
                        .and_then(|nanos| seconds.checked_add(nanos))
                })
                .ok_or(Error::Contract("monotonic timestamp overflow"))?;
            publisher.send("testJoystick", &encode(values, timestamp))?;
            frame = frame.saturating_add(1);
            let next = deadline.unwrap_or_else(|| Instant::now() + Duration::from_millis(10));
            if let Some(remaining) = next.checked_duration_since(Instant::now()) {
                thread::sleep(remaining);
            }
            deadline = Some(next + Duration::from_millis(10));
            if options.frames.is_some_and(|limit| frame >= limit.get()) {
                break;
            }
        }
        Ok(())
    })();
    stop.store(true, Ordering::Relaxed);
    let reader_result = worker
        .join()
        .map_err(|_| Error::Contract("joystick input thread panicked"))?;
    result.and(reader_result)
}
