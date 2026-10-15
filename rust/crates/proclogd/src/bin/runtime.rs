use openpilot_msgq::Publisher;
use openpilot_proclogd::clock::Clock;
use openpilot_proclogd::{cadence::Cadence, wire::encode_snapshot, PROC_LOG_QUEUE_SIZE};
use openpilot_runtime_core::procfs::{Collector, SamplingClock};
use std::{
    env,
    num::NonZeroU64,
    path::PathBuf,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

struct Options {
    frames: Option<NonZeroU64>,
    root: PathBuf,
}

#[derive(Debug, thiserror::Error)]
enum EntryError {
    #[error("{0}")]
    Arguments(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    #[error(transparent)]
    Parse(#[from] std::num::ParseIntError),
    #[error(transparent)]
    Messaging(#[from] openpilot_msgq::Error),
    #[error(transparent)]
    Encode(#[from] openpilot_proclogd::wire::Error),
}

fn options() -> Result<Option<Options>, EntryError> {
    let mut args = env::args_os().skip(1);
    let mut frames = None;
    let mut root = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help") => {
                println!("openpilot-proclogd-runtime [--frames N] [--proc-root DIR]\n\nPublishes canonical procLog at the original 0.5 Hz cadence.\nRuns continuously by default; SIGINT/SIGTERM request orderly shutdown.\nUses the original OPENPILOT_PREFIX namespace. --proc-root is for host QA.\nProduction manager selection is unchanged.");
                return Ok(None);
            }
            Some("--frames") if frames.is_none() => {
                frames = Some(
                    args.next()
                        .ok_or(EntryError::Arguments("missing frame count"))?
                        .to_str()
                        .ok_or(EntryError::Arguments("invalid frame count"))?
                        .parse::<NonZeroU64>()?,
                );
            }
            Some("--proc-root") if root.is_none() => {
                root = Some(PathBuf::from(
                    args.next()
                        .ok_or(EntryError::Arguments("missing proc root"))?,
                ));
            }
            _ => {
                return Err(EntryError::Arguments(
                    "unknown or duplicate option; see --help",
                ))
            }
        }
    }
    Ok(Some(Options {
        frames,
        root: root.unwrap_or_else(|| PathBuf::from("/proc")),
    }))
}

fn timestamp() -> Result<u64, EntryError> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    u64::try_from(now.tv_sec)?
        .checked_mul(1_000_000_000)
        .and_then(|seconds| seconds.checked_add(u64::try_from(now.tv_nsec).ok()?))
        .ok_or(EntryError::Arguments("monotonic timestamp overflow"))
}

fn run(options: Options) -> Result<(), EntryError> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let mut publisher = Publisher::for_runtime("procLog", PROC_LOG_QUEUE_SIZE)?;
    let mut collector = Collector::new(
        &options.root,
        NonZeroU64::new(rustix::param::clock_ticks_per_second())
            .ok_or(EntryError::Arguments("invalid clock tick rate"))?,
        NonZeroU64::new(u64::try_from(rustix::param::page_size())?)
            .ok_or(EntryError::Arguments("invalid page size"))?,
    );
    let clock = Clock;
    let mut next_publish = 0;
    let mut cadence = Cadence;
    let mut remaining = options.frames.map(NonZeroU64::get);
    while !stop.load(Ordering::Relaxed) {
        let timestamp = timestamp()?;
        if timestamp >= next_publish {
            let snapshot = collector.snapshot_with_clock(&clock)?;
            if !snapshot.warnings.is_empty() {
                eprintln!("proclogd: collection warnings: {:?}", snapshot.warnings);
            }
            if stop.load(Ordering::Relaxed) {
                break;
            }
            publisher.send(&encode_snapshot(&snapshot, timestamp)?)?;
            if let Some(frames) = &mut remaining {
                *frames -= 1;
                if *frames == 0 {
                    break;
                }
            }
            next_publish =
                u64::try_from(cadence.deadline(Duration::from_nanos(timestamp)).as_nanos())?;
        }
        collector.smaps_step(&clock);
        let wait = next_publish
            .min(collector.smaps_next_step_ns())
            .saturating_sub(clock.monotonic_ns());
        thread::sleep(Duration::from_nanos(wait).min(Duration::from_millis(20)));
    }
    Ok(())
}

fn main() -> ExitCode {
    match openpilot_managed_entry::runtime("openpilot.system.proclogd", "proclogd", || {
        options()
            .and_then(|options| options.map_or(Ok(()), run))
            .map_err(openpilot_managed_entry::StepError::raised)
    }) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("proclogd: {error}");
            ExitCode::FAILURE
        }
    }
}
