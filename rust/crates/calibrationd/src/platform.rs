use crate::{Error, Limits};
use std::{fs, path::Path};

pub fn configure() -> Result<Limits, Error> {
    if !Path::new("/TICI").is_file() {
        return Ok(Limits::standard());
    }
    // SAFETY: sched_param is initialized before the kernel reads it; pid0 denotes this process.
    let mut settings: libc::sched_param = unsafe { std::mem::zeroed() };
    settings.sched_priority = 5;
    // SAFETY: settings is a valid borrowed sched_param for this synchronous syscall.
    if unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &settings) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut cores = rustix::thread::CpuSet::new();
    for core in 0..4 {
        cores.set(core);
    }
    rustix::thread::sched_setaffinity(None, &cores).map_err(std::io::Error::from)?;
    let model = fs::read_to_string("/sys/firmware/devicetree/base/model")?;
    Ok(
        if model.trim_matches('\0').rsplit("comma ").next() == Some("mici") {
            Limits::mici()
        } else {
            Limits::standard()
        },
    )
}

pub fn timestamp() -> Result<u64, Error> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    u64::try_from(now.tv_sec)
        .ok()
        .and_then(|seconds| seconds.checked_mul(1_000_000_000))
        .and_then(|seconds| seconds.checked_add(u64::try_from(now.tv_nsec).ok()?))
        .ok_or(Error::Contract("monotonic timestamp overflow"))
}
