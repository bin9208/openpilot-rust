use openpilot_msgq::Publisher;
use openpilot_proclogd::{cadence::Cadence, wire::encode_snapshot, PROC_LOG_QUEUE_SIZE};
use openpilot_runtime_core::procfs::Collector;
use std::{
    env,
    error::Error,
    num::NonZeroU64,
    path::PathBuf,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

struct Options {
    frames: Option<NonZeroU64>,
    root: PathBuf,
}

fn options() -> Result<Option<Options>, Box<dyn Error>> {
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
                        .ok_or("missing frame count")?
                        .to_str()
                        .ok_or("invalid frame count")?
                        .parse::<NonZeroU64>()?,
                );
            }
            Some("--proc-root") if root.is_none() => {
                root = Some(PathBuf::from(args.next().ok_or("missing proc root")?));
            }
            _ => return Err("unknown or duplicate option; see --help".into()),
        }
    }
    Ok(Some(Options {
        frames,
        root: root.unwrap_or_else(|| PathBuf::from("/proc")),
    }))
}

fn timestamp() -> Result<u64, Box<dyn Error>> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    u64::try_from(now.tv_sec)?
        .checked_mul(1_000_000_000)
        .and_then(|seconds| seconds.checked_add(u64::try_from(now.tv_nsec).ok()?))
        .ok_or_else(|| "monotonic timestamp overflow".into())
}

fn run(options: Options) -> Result<(), Box<dyn Error>> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let mut publisher = Publisher::for_runtime("procLog", PROC_LOG_QUEUE_SIZE)?;
    let mut collector = Collector::new(
        &options.root,
        NonZeroU64::new(rustix::param::clock_ticks_per_second())
            .ok_or("invalid clock tick rate")?,
        NonZeroU64::new(u64::try_from(rustix::param::page_size())?).ok_or("invalid page size")?,
    );
    let epoch = Instant::now();
    let mut cadence = Cadence::default();
    let mut remaining = options.frames.map(NonZeroU64::get);
    while !stop.load(Ordering::Relaxed) {
        let timestamp = timestamp()?;
        let snapshot = collector.snapshot()?;
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
        let now = epoch.elapsed();
        let deadline = cadence.deadline(now);
        if now > deadline {
            eprintln!(
                "proclogd lagging by {:.2} ms",
                (now - deadline).as_secs_f64() * 1000.0
            );
        }
        while !stop.load(Ordering::Relaxed) {
            let wait = deadline.saturating_sub(epoch.elapsed());
            if wait.is_zero() {
                break;
            }
            // Bound shutdown latency without moving the source's absolute deadline.
            thread::sleep(wait.min(Duration::from_millis(20)));
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match options().and_then(|options| options.map_or(Ok(()), run)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("proclogd: {error}");
            ExitCode::FAILURE
        }
    }
}
