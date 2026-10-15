//! Apply source scheduling to all currently existing observer threads.
use rustix::{
    process::{Pid, setpriority_process},
    thread::{CpuSet, sched_getaffinity, sched_setaffinity},
};
use std::{fs, io};
#[allow(unsafe_code)]
fn set_other(tid: i32) -> io::Result<()> {
    let param = libc::sched_param { sched_priority: 0 };
    // SAFETY: tid is a parsed positive Linux task ID; the initialized parameter
    // is borrowed for this synchronous syscall and contains no pointers.
    if unsafe { libc::sched_setscheduler(tid, libc::SCHED_OTHER, &param) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub fn configure() -> io::Result<()> {
    let current = sched_getaffinity(None)?;
    let mut little = CpuSet::new();
    for core in 0..4 {
        if current.is_set(core) {
            little.set(core);
        }
    }
    if !(0..4).any(|core| little.is_set(core)) {
        return Err(io::Error::other(
            "no permitted little CPU for signal observer",
        ));
    }
    for task in fs::read_dir("/proc/self/task")? {
        let tid = task?
            .file_name()
            .to_string_lossy()
            .parse::<i32>()
            .map_err(io::Error::other)?;
        let pid = Pid::from_raw(tid).ok_or_else(|| io::Error::other("invalid task ID"))?;
        let apply = || -> io::Result<()> {
            sched_setaffinity(Some(pid), &little)?;
            set_other(tid)?;
            setpriority_process(Some(pid), 19)?;
            Ok(())
        };
        if let Err(error) = apply() {
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error);
            }
        }
    }
    Ok(())
}
