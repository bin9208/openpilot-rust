use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::event;
use openpilot_runtime_core::procfs::Snapshot;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} does not fit the canonical procLog field")]
    OutOfRange(&'static str),
}

fn seconds<T: ToPrimitive>(ticks: T, hz: f64) -> Result<f32, Error> {
    (ticks.to_f64().ok_or(Error::OutOfRange("clock ticks"))? / hz)
        .to_f32()
        .ok_or(Error::OutOfRange("clock seconds"))
}

/// Encode the complete source ProcLog contract with the caller's monotonic timestamp.
///
/// # Errors
/// Returns a range error instead of wrapping a value narrower in the wire schema.
pub fn encode_snapshot(snapshot: &Snapshot, log_mono_time: u64) -> Result<Vec<u8>, Error> {
    let hz = snapshot
        .ticks_per_second
        .get()
        .to_f64()
        .ok_or(Error::OutOfRange("clock rate"))?;
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_log_mono_time(log_mono_time);
    event.set_valid(true);
    let mut log = event.init_proc_log();
    let process_count =
        u32::try_from(snapshot.processes.len()).map_err(|_| Error::OutOfRange("process count"))?;
    let mut processes = log.reborrow().init_procs(process_count);
    for (index, process) in snapshot.processes.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| Error::OutOfRange("process index"))?;
        let mut target = processes.reborrow().get(index);
        let stat = &process.stat;
        target.set_pid(i32::try_from(stat.pid).map_err(|_| Error::OutOfRange("pid"))?);
        target.set_name(stat.name.as_str());
        target.set_state(stat.state);
        target.set_ppid(i32::try_from(stat.ppid).map_err(|_| Error::OutOfRange("ppid"))?);
        target.set_cpu_user(seconds(stat.user_ticks, hz)?);
        target.set_cpu_system(seconds(stat.system_ticks, hz)?);
        target.set_cpu_children_user(seconds(stat.children_user_ticks, hz)?);
        target.set_cpu_children_system(seconds(stat.children_system_ticks, hz)?);
        target.set_priority(stat.priority);
        target.set_nice(i32::try_from(stat.nice).map_err(|_| Error::OutOfRange("nice"))?);
        target.set_num_threads(
            i32::try_from(stat.threads).map_err(|_| Error::OutOfRange("threads"))?,
        );
        target.set_start_time(
            stat.start_ticks
                .to_f64()
                .ok_or(Error::OutOfRange("start ticks"))?
                / hz,
        );
        target.set_mem_vms(stat.virtual_bytes);
        target.set_mem_rss(process.resident_bytes);
        target.set_processor(
            i32::try_from(stat.processor).map_err(|_| Error::OutOfRange("processor"))?,
        );
        target.set_exe(process.exe.as_str());
        target.set_mem_pss(process.proportional.pss);
        target.set_mem_pss_anon(process.proportional.anon);
        target.set_mem_pss_shmem(process.proportional.shared);
        let length = u32::try_from(process.cmdline.len())
            .map_err(|_| Error::OutOfRange("argument count"))?;
        let mut cmdline = target.init_cmdline(length);
        for (index, arg) in process.cmdline.iter().enumerate() {
            let index = u32::try_from(index).map_err(|_| Error::OutOfRange("argument index"))?;
            cmdline.set(index, arg.as_str());
        }
    }
    let cpu_count =
        u32::try_from(snapshot.cpu_times.len()).map_err(|_| Error::OutOfRange("cpu count"))?;
    let mut cpus = log.reborrow().init_cpu_times(cpu_count);
    for (index, cpu) in snapshot.cpu_times.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| Error::OutOfRange("cpu index"))?;
        let mut target = cpus.reborrow().get(index);
        target.set_cpu_num(i64::from(cpu.cpu));
        let [user, nice, system, idle, iowait, irq, softirq] = cpu.ticks;
        target.set_user(seconds(user, hz)?);
        target.set_nice(seconds(nice, hz)?);
        target.set_system(seconds(system, hz)?);
        target.set_idle(seconds(idle, hz)?);
        target.set_iowait(seconds(iowait, hz)?);
        target.set_irq(seconds(irq, hz)?);
        target.set_softirq(seconds(softirq, hz)?);
    }
    let source = &snapshot.memory;
    let mut memory = log.init_mem();
    memory.set_total(source.total);
    memory.set_free(source.free);
    memory.set_available(source.available);
    memory.set_buffers(source.buffers);
    memory.set_cached(source.cached);
    memory.set_active(source.active);
    memory.set_inactive(source.inactive);
    memory.set_shared(source.shared);
    Ok(capnp::serialize::write_message_to_words(&message))
}
