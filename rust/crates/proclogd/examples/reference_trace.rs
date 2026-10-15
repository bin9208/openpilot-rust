use openpilot_proclogd::wire::encode_snapshot;
use openpilot_runtime_core::procfs::{Collector, SamplingClock};
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
        let line = line?;
        let mut fields = line.split_whitespace();
        let timestamp: u64 = fields.next().ok_or("missing timestamp")?.parse()?;
        struct Clock(u64);
        impl SamplingClock for Clock {
            fn monotonic_ns(&self) -> u64 {
                self.0
            }
            fn thread_cpu_ns(&self) -> u64 {
                0
            }
        }
        if fields.next() == Some("step") {
            collector.smaps_step(&Clock(timestamp));
        }
        let bytes = encode_snapshot(&collector.snapshot_at(timestamp)?, timestamp)?;
        stdout.write_all(&u64::try_from(bytes.len())?.to_le_bytes())?;
        stdout.write_all(&bytes)?;
        stdout.flush()?;
    }
    Ok(())
}
