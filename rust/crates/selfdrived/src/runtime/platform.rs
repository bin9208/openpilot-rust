use crate::controller::Error;
use std::path::Path;

pub fn configure() -> Result<(), Error> {
    if !cfg!(target_os = "linux") || !Path::new("/TICI").is_file() {
        return Ok(());
    }
    let settings = libc::sched_param { sched_priority: 53 };
    // SAFETY: the initialized sched_param remains borrowed for this synchronous call;
    // pid zero addresses only the calling process. The kernel copies the parameter.
    if unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &settings) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut cores = rustix::thread::CpuSet::new();
    cores.set(6);
    rustix::thread::sched_setaffinity(None, &cores).map_err(std::io::Error::from)?;
    Ok(())
}
