use openpilot_logmessaged::{
    options::Options,
    wire::{packet, Topic},
    LogFiles, RotationSettings,
};
use openpilot_messaging::runtime::PubMaster;
use std::{
    error::Error,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

fn timestamp() -> Result<u64, Box<dyn Error>> {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let seconds = u64::try_from(time.tv_sec)?;
    let nanos = u32::try_from(time.tv_nsec)?;
    let timestamp = seconds
        .checked_mul(1_000_000_000)
        .and_then(|value| value.checked_add(u64::from(nanos)))
        .ok_or("monotonic timestamp overflow")?;
    Ok(timestamp)
}

fn run(options: Options) -> Result<(), Box<dyn Error>> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    std::fs::create_dir_all(&options.root)?;
    let mut files = LogFiles::new(
        &options.root.join("swaglog"),
        RotationSettings::default(),
        || {
            let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
            // Python time.monotonic() exposes a Float64 count of seconds.
            now.tv_sec as f64 + now.tv_nsec as f64 / 1e9
        },
    )?;
    let context = zmq::Context::new();
    let socket = context.socket(zmq::PULL)?;
    socket.set_rcvtimeo(100)?;
    socket.bind(&options.endpoint)?;
    let mut publisher = PubMaster::for_runtime(&[Topic::Log.name(), Topic::Error.name()])?;
    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut frames = 0_u64;
        while !stop.load(Ordering::Relaxed) {
            let parts = match socket.recv_multipart(0) {
                Ok(parts) => parts,
                Err(zmq::Error::EAGAIN | zmq::Error::EINTR) => continue,
                Err(error) => return Err(error.into()),
            };
            let bytes: Vec<u8> = parts.into_iter().flatten().collect();
            let (&level, raw) = bytes
                .split_first()
                .ok_or("empty log record has no level byte")?;
            let record = String::from_utf8_lossy(raw);
            if level >= 20 {
                if let Err(error) = files.emit(&record) {
                    // BaseRotatingHandler.emit catches formatting/rotation/write errors;
                    // report the failure without skipping either publication gate.
                    eprintln!("logmessaged: file handler error: {error}");
                }
            }
            let characters = record.chars().count();
            if characters > 2 * 1024 * 1024 {
                println!("WARNING: log too big to publish {characters}");
                println!("{}", record.chars().take(100).collect::<String>());
            } else {
                publisher.send(
                    Topic::Log.name(),
                    &packet(&record, Topic::Log, timestamp()?),
                )?;
                if level >= 40 {
                    publisher.send(
                        Topic::Error.name(),
                        &packet(&record, Topic::Error, timestamp()?),
                    )?;
                }
            }
            frames += 1;
            if options.frames == Some(frames) {
                break;
            }
        }
        Ok(())
    })();
    drop(socket);
    drop(context);
    files.close()?;
    result
}
fn main() -> ExitCode {
    match Options::parse(std::env::args_os().skip(1))
        .map_err(|error| -> Box<dyn Error> { Box::new(error) })
        .and_then(|options| options.map_or(Ok(()), run))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("logmessaged: {error}");
            ExitCode::FAILURE
        }
    }
}
