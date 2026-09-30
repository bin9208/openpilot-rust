use crate::Error;
use std::path::Path;
pub fn configure() -> Result<(), Error> {
    if Path::new("/TICI").is_file() {
        let settings = libc::sched_param { sched_priority: 5 };
        // SAFETY: settings is initialized; pid0 configures the current process.
        if unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &settings) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut cores = rustix::thread::CpuSet::new();
        for core in 0..4 {
            cores.set(core);
        }
        rustix::thread::sched_setaffinity(None, &cores).map_err(std::io::Error::from)?;
    }
    Ok(())
}
pub fn timestamp() -> Result<u64, Error> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    u64::try_from(now.tv_sec)
        .ok()
        .and_then(|s| s.checked_mul(1_000_000_000))
        .and_then(|s| s.checked_add(u64::try_from(now.tv_nsec).ok()?))
        .ok_or(Error::Contract("monotonic timestamp overflow"))
}
