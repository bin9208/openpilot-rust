use openpilot_proclogd::wire::encode_snapshot;
use openpilot_runtime_core::procfs::Collector;
use std::{
    env,
    error::Error,
    io::{self, BufRead, Write},
    num::NonZeroU64,
    path::Path,
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--queue-size") {
        println!("{}", openpilot_proclogd::PROC_LOG_QUEUE_SIZE);
        return Ok(());
    }
    if args.len() != 4 {
        return Err("usage: reference_trace PROC_ROOT TICKS_PER_SECOND PAGE_SIZE".into());
    }
    let mut collector = Collector::new(
        Path::new(&args[1]),
        args[2].parse::<NonZeroU64>()?,
        args[3].parse::<NonZeroU64>()?,
    );
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let timestamp: u64 = line?.parse()?;
        let bytes = encode_snapshot(&collector.snapshot()?, timestamp)?;
        stdout.write_all(&u64::try_from(bytes.len())?.to_le_bytes())?;
        stdout.write_all(&bytes)?;
        stdout.flush()?;
    }
    Ok(())
}
