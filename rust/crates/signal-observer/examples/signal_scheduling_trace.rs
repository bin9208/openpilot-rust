use std::{
    sync::{Arc, Barrier},
    thread,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let barrier = Arc::new(Barrier::new(4));
    let handles = (0..3)
        .map(|_| {
            let b = barrier.clone();
            thread::spawn(move || {
                b.wait();
                b.wait();
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    openpilot_signal_observer::scheduling::configure()?;
    let mut tasks = Vec::new();
    for entry in std::fs::read_dir("/proc/self/task")? {
        let tid = entry?.file_name().to_string_lossy().parse::<i32>()?;
        let pid = rustix::process::Pid::from_raw(tid).ok_or("invalid thread")?;
        let affinity = rustix::thread::sched_getaffinity(Some(pid))?;
        let stat = std::fs::read_to_string(format!("/proc/self/task/{tid}/stat"))?;
        let fields = stat[stat.rfind(')').ok_or("bad stat")? + 2..]
            .split_whitespace()
            .collect::<Vec<_>>();
        tasks.push(serde_json::json!({"affinity":(0..rustix::thread::CpuSet::MAX_CPU).filter(|v|affinity.is_set(*v)).collect::<Vec<_>>(),"nice":rustix::process::getpriority_process(Some(pid))?,"policy":fields[38].parse::<i32>()?,"realtime_priority":fields[37].parse::<i32>()?}));
    }
    barrier.wait();
    for h in handles {
        h.join().map_err(|_| "thread panic")?;
    }
    println!("{}", serde_json::to_string(&tasks)?);
    Ok(())
}
