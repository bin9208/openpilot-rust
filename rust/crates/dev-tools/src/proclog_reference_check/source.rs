//! The original checker's fixed procfs input and source collector arithmetic.
//! Generated cereal bindings only supply wire types/rounding, not a collector.
use super::{project_event, Result};
use openpilot_cereal::log_capnp::event;
use serde_json::Value;
use std::{fs, os::unix::fs::symlink, path::Path};
const INITIAL: &str = "Pss: 50 kB\nPss_Anon: 30 kB\nPss_Shmem: 10 kB\n";
pub fn prepare(root: &Path, fallback: bool) -> Result<()> {
    fs::write(
        root.join("stat"),
        "cpu 9 9 9 9 9 9 9\ncpu0 100 2 30 400 5 6 7\ncpu7 200 3 40 500 6 7 8\nintr 44\n",
    )?;
    fs::write(root.join("meminfo"), "MemTotal: 128 kB\nMemFree: 16 kB\nMemAvailable: 32 kB\nBuffers: 2 kB\nCached: 4 kB\nActive: 5 kB\nInactive: 6 kB\nShmem: 7 kB\n")?;
    for (pid, rss) in [(123, 2000), (456, 1)] {
        make_process(root, pid, rss)?;
        if fallback {
            let directory = root.join(pid.to_string());
            fs::remove_file(directory.join("smaps_rollup"))?;
            fs::write(
                directory.join("smaps"),
                [
                    b"1000-2000 r--p 00000000 00:00 0 /synthetic/\xff\n".as_slice(),
                    INITIAL.as_bytes(),
                ]
                .concat(),
            )?;
        }
    }
    Ok(())
}
/// Original shared make_process fixture, independent of candidate collection.
pub fn make_process(root: &Path, pid: u32, rss: u64) -> Result<()> {
    let directory = root.join(pid.to_string());
    fs::create_dir(&directory)?;
    let mut fields = vec!["0".to_owned(); 50];
    for (index, value) in [
        (0, "S"),
        (1, "1"),
        (11, "125"),
        (12, "27"),
        (13, "-3"),
        (14, "4"),
        (15, "20"),
        (16, "-5"),
        (17, "3"),
        (19, "501"),
        (20, "16777216"),
        (36, "6"),
    ] {
        fields[index] = value.into();
    }
    fields[21] = rss.to_string();
    fs::write(
        directory.join("stat"),
        format!("{pid} (worker ) (a) {}", fields.join(" ")),
    )?;
    fs::write(directory.join("cmdline"), b"worker\0--test\0\xe1\x90\xff\0")?;
    symlink("/synthetic/bin/worker", directory.join("exe"))?;
    fs::write(directory.join("smaps_rollup"), INITIAL)?;
    Ok(())
}
pub fn advance(root: &Path, fallback: bool, cycle: usize) -> Result<()> {
    if cycle == 1 {
        fs::write(
            root.join("123")
                .join(if fallback { "smaps" } else { "smaps_rollup" }),
            "Pss: 99 kB\nPss_Anon: 80 kB\nPss_Shmem: 4 kB\n",
        )?;
    }
    Ok(())
}
pub fn expected_event(cycle: usize, ticks: u64, page: u64) -> Result<Value> {
    if ticks == 0 || page == 0 {
        return Err("nonpositive source sysconf value".into());
    }
    let hz = ticks as f64;
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(1000 + u64::try_from(cycle)?);
    event.set_valid(true);
    let mut log = event.init_proc_log();
    let mut processes = log.reborrow().init_procs(2);
    for (index, (pid, rss)) in [(123, 2000_u64), (456, 1)].into_iter().enumerate() {
        let mut process = processes.reborrow().get(index as u32);
        process.set_pid(pid);
        process.set_name("worker ) (a");
        process.set_state(b'S');
        process.set_ppid(1);
        process.set_cpu_user((125.0 / hz) as f32);
        process.set_cpu_system((27.0 / hz) as f32);
        process.set_cpu_children_user((-3.0 / hz) as f32);
        process.set_cpu_children_system((4.0 / hz) as f32);
        process.set_priority(20);
        process.set_nice(-5);
        process.set_num_threads(3);
        process.set_start_time(501.0 / hz);
        process.set_mem_vms(16777216);
        let resident = rss.checked_mul(page).ok_or("source RSS overflow")?;
        process.set_mem_rss(resident);
        process.set_processor(6);
        process.set_exe("/synthetic/bin/worker");
        if resident > 5 * 1024 * 1024 {
            let (pss, anon, shared) = if pid == 123 && cycle >= 20 {
                (99, 80, 4)
            } else {
                (50, 30, 10)
            };
            process.set_mem_pss(pss * 1024);
            process.set_mem_pss_anon(anon * 1024);
            process.set_mem_pss_shmem(shared * 1024);
        }
        let mut command = process.init_cmdline(3);
        for (index, value) in ["worker", "--test", "\u{fffd}\u{fffd}"]
            .into_iter()
            .enumerate()
        {
            command.set(index as u32, value);
        }
    }
    let mut cpus = log.reborrow().init_cpu_times(2);
    for (index, (number, fields)) in [
        (0, [100., 2., 30., 400., 5., 6., 7.]),
        (7, [200., 3., 40., 500., 6., 7., 8.]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut cpu = cpus.reborrow().get(index as u32);
        cpu.set_cpu_num(number);
        cpu.set_user((fields[0] / hz) as f32);
        cpu.set_nice((fields[1] / hz) as f32);
        cpu.set_system((fields[2] / hz) as f32);
        cpu.set_idle((fields[3] / hz) as f32);
        cpu.set_iowait((fields[4] / hz) as f32);
        cpu.set_irq((fields[5] / hz) as f32);
        cpu.set_softirq((fields[6] / hz) as f32);
    }
    let mut memory = log.init_mem();
    memory.set_total(128 * 1024);
    memory.set_free(16 * 1024);
    memory.set_available(32 * 1024);
    memory.set_buffers(2 * 1024);
    memory.set_cached(4 * 1024);
    memory.set_active(5 * 1024);
    memory.set_inactive(6 * 1024);
    memory.set_shared(7 * 1024);
    project_event(&capnp::serialize::write_message_to_words(&message))
}
