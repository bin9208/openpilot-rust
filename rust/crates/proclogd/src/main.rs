use openpilot_cereal::log_capnp::event;
use openpilot_msgq::{Publisher, Subscriber};
use openpilot_params::{Params, CLEAR_ON_MANAGER_START};
use openpilot_proclogd::{wire::encode_snapshot, PROC_LOG_QUEUE_SIZE};
use openpilot_runtime_core::procfs::Collector;
use std::{
    env,
    error::Error,
    fs::{self, OpenOptions},
    io::{self, Write},
    num::NonZeroU64,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

const HELP: &str = "openpilot-proclogd 0.1.0\nBounded experimental procLog producer; production selection is unchanged.\n\nModes (choose one):\n  --stdout                 Write concatenated canonical Event messages\n  --output-dir DIR         Create procLog-NNNN.capnp files without overwriting\n  --publish                Publish to procLog in an isolated namespace\n  --self-test              Check isolated msgq and temporary Params storage\n\nOptions:\n  --frames N               1..300 messages (default 10)\n  --interval-ms N          0..60000 milliseconds (default 2000 = 0.5 Hz)\n  --proc-root DIR          procfs root (default /proc)\n  --help                   Show this help\n  --version                Show version\n\nIPC requires OPENPILOT_PREFIX=rust-probe-NAME and an existing\n/dev/shm/msgq_rust-probe-NAME directory. Never use a production namespace.\n";

enum Mode {
    Stdout,
    Files(PathBuf),
    Publish,
    SelfTest,
}
struct Options {
    mode: Mode,
    root: PathBuf,
    frames: u32,
    interval: Duration,
}

fn options() -> Result<Option<Options>, Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let mut mode = None;
    let mut root = PathBuf::from("/proc");
    let mut frames = 10;
    let mut milliseconds = 2000;
    while let Some(arg) = args.next() {
        let selection = match arg.as_str() {
            "--help" => {
                print!("{HELP}");
                return Ok(None);
            }
            "--version" => {
                println!("openpilot-proclogd {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--stdout" => Some(Mode::Stdout),
            "--publish" => Some(Mode::Publish),
            "--self-test" => Some(Mode::SelfTest),
            "--output-dir" => Some(Mode::Files(
                args.next().ok_or("missing output directory")?.into(),
            )),
            "--proc-root" => {
                root = args.next().ok_or("missing proc root")?.into();
                None
            }
            "--frames" => {
                frames = args.next().ok_or("missing frame count")?.parse()?;
                None
            }
            "--interval-ms" => {
                milliseconds = args.next().ok_or("missing interval")?.parse()?;
                None
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        };
        if let Some(selection) = selection {
            if mode.is_some() {
                return Err("choose exactly one output mode".into());
            }
            mode = Some(selection);
        }
    }
    if !(1..=300).contains(&frames) || milliseconds > 60_000 {
        return Err("frames or interval is outside the allowed range".into());
    }
    Ok(Some(Options {
        mode: mode.ok_or("choose an output mode; see --help")?,
        root,
        frames,
        interval: Duration::from_millis(milliseconds),
    }))
}

fn collector(root: &std::path::Path) -> Result<Collector, Box<dyn Error>> {
    Ok(Collector::new(
        root,
        NonZeroU64::new(rustix::param::clock_ticks_per_second())
            .ok_or("invalid clock tick rate")?,
        NonZeroU64::new(u64::try_from(rustix::param::page_size())?).ok_or("invalid page size")?,
    ))
}

fn timestamp() -> Result<u64, Box<dyn Error>> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    u64::try_from(now.tv_sec)?
        .checked_mul(1_000_000_000)
        .and_then(|seconds| seconds.checked_add(u64::try_from(now.tv_nsec).ok()?))
        .ok_or_else(|| "monotonic timestamp overflow".into())
}

fn self_test(root: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let params = Params::open(temporary.path(), "probe")?;
    let payload = [0, 255, 1, 0, 42];
    params.put("CarParams", &payload)?;
    if params.get("CarParams")?.as_deref() != Some(payload.as_slice()) {
        return Err("Params binary mismatch".into());
    }
    params.clear(CLEAR_ON_MANAGER_START)?;
    if params.get("CarParams")?.is_some() {
        return Err("Params clear failed".into());
    }
    let mut publisher = Publisher::with_capacity("rustSelfTest", PROC_LOG_QUEUE_SIZE)?;
    let mut subscriber = Subscriber::with_capacity("rustSelfTest", false, PROC_LOG_QUEUE_SIZE)?;
    let timestamp = timestamp()?;
    let snapshot = collector(root)?.snapshot()?;
    let bytes = encode_snapshot(&snapshot, timestamp)?;
    publisher.send(&bytes)?;
    let received = subscriber
        .receive(Duration::from_secs(1))?
        .ok_or("msgq receive timeout")?;
    if received != bytes {
        return Err("msgq payload mismatch".into());
    }
    let reader = capnp::serialize::read_message(received.as_slice(), Default::default())?;
    let event = reader.get_root::<event::Reader>()?;
    if !event.get_valid() || !matches!(event.which()?, event::ProcLog(_)) {
        return Err("invalid procLog event".into());
    }
    println!(
        "PASS: temporary Params, isolated msgq, canonical procLog ({} processes)",
        snapshot.processes.len()
    );
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let Some(options) = options()? else {
        return Ok(());
    };
    if matches!(options.mode, Mode::SelfTest) {
        return self_test(&options.root);
    }
    let mut publisher = if matches!(options.mode, Mode::Publish) {
        Some(Publisher::with_capacity("procLog", PROC_LOG_QUEUE_SIZE)?)
    } else {
        None
    };
    if let Some(publisher) = &mut publisher {
        let limit = Instant::now() + Duration::from_secs(3);
        while !publisher.readers_caught_up() {
            if Instant::now() >= limit {
                return Err("no isolated subscriber connected within 3 seconds".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    if let Mode::Files(directory) = &options.mode {
        fs::create_dir_all(directory)?;
    }
    let mut collector = collector(&options.root)?;
    let mut output = io::stdout().lock();
    let mut deadline = Instant::now();
    for frame in 0..options.frames {
        thread::sleep(deadline.saturating_duration_since(Instant::now()));
        let timestamp = timestamp()?;
        let snapshot = collector.snapshot()?;
        if !snapshot.warnings.is_empty() {
            eprintln!("collection warnings: {:?}", snapshot.warnings);
        }
        let bytes = encode_snapshot(&snapshot, timestamp)?;
        match &options.mode {
            Mode::Stdout => {
                output.write_all(&bytes)?;
                output.flush()?;
            }
            Mode::Files(directory) => {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(directory.join(format!("procLog-{frame:04}.capnp")))?;
                file.write_all(&bytes)?;
                file.sync_all()?;
            }
            Mode::Publish => publisher
                .as_mut()
                .ok_or("missing publisher")?
                .send(&bytes)?,
            Mode::SelfTest => unreachable!(),
        }
        deadline += options.interval;
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe) =>
        {
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("openpilot-proclogd: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
