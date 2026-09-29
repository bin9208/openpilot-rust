//! One-shot Linux diagnostic. Not a replacement for production proclogd.
#[cfg(target_os = "linux")]
mod linux {
    use openpilot_runtime_core::proc_stat::{cpu_percent, ProcessStat};
    use std::{
        collections::HashMap,
        error::Error,
        fs,
        process::Command,
        time::{Duration, Instant},
    };

    fn snapshot() -> std::io::Result<HashMap<u32, (ProcessStat, Instant)>> {
        let mut result = HashMap::new();
        for entry in fs::read_dir("/proc")? {
            let Ok(entry) = entry else { continue };
            if entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
                .is_none()
            {
                continue;
            }
            // Processes may exit or become unreadable between enumeration and read.
            let Ok(text) = fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            if let Some(p) = ProcessStat::parse(&text) {
                result.insert(p.pid, (p, Instant::now()));
            }
        }
        Ok(result)
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let interval = match args.as_slice() {
            [] => 1000,
            [s] => s
                .parse::<u64>()
                .ok()
                .filter(|n| (1..=60000).contains(n))
                .ok_or("interval must be an integer in 1..60000 milliseconds")?,
            _ => return Err("usage: cpu-sample [interval-ms]".into()),
        };
        let clock = Command::new("getconf").arg("CLK_TCK").output()?;
        if !clock.status.success() {
            return Err("getconf CLK_TCK failed".into());
        }
        let ticks: u64 = std::str::from_utf8(&clock.stdout)?.trim().parse()?;
        if ticks == 0 {
            return Err("CLK_TCK must be positive".into());
        }
        let before = snapshot()?;
        std::thread::sleep(Duration::from_millis(interval));
        let after = snapshot()?;
        let mut rows = Vec::new();
        for (pid, (p, now)) in &after {
            if let Some((previous, then)) = before.get(pid) {
                if let Some(cpu) = cpu_percent(previous, p, now.duration_since(*then), ticks) {
                    rows.push((p, cpu));
                }
            }
        }
        rows.sort_by(|(a, x), (b, y)| y.total_cmp(x).then(a.pid.cmp(&b.pid)));
        println!("pid\tname\tcpu_percent_one_core\tlast_processor\tthreads");
        for (p, cpu) in rows {
            let name = p
                .name
                .replace('\\', "\\\\")
                .replace('\t', "\\t")
                .replace('\n', "\\n")
                .replace('\r', "\\r");
            println!(
                "{}\t{}\t{:.3}\t{}\t{}",
                p.pid, name, cpu, p.processor, p.threads
            );
        }
        Ok(())
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    if let Err(error) = linux::run() {
        eprintln!("cpu-sample: {error}");
        std::process::exit(1);
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("cpu-sample requires Linux /proc");
        std::process::exit(1);
    }
}
