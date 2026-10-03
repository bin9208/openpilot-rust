use crate::controller::Error;
use std::path::Path;

type SetScheduler =
    unsafe extern "C" fn(libc::pid_t, libc::c_int, *const libc::sched_param) -> libc::c_int;

fn fifo(set: SetScheduler) -> Result<(), Error> {
    // SAFETY: sched_param has integer/time fields; zero is valid for GNU and musl's reserved layout.
    let mut settings: libc::sched_param = unsafe { std::mem::zeroed() };
    settings.sched_priority = 53;
    // SAFETY: the initialized sched_param remains borrowed for this synchronous call;
    // pid zero addresses only the calling process. The kernel copies the parameter.
    if unsafe { set(0, libc::SCHED_FIFO, &settings) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub fn configure() -> Result<(), Error> {
    if !cfg!(target_os = "linux") || !Path::new("/TICI").is_file() {
        return Ok(());
    }
    fifo(libc::sched_setscheduler)?;
    let mut cores = rustix::thread::CpuSet::new();
    cores.set(6);
    rustix::thread::sched_setaffinity(None, &cores).map_err(std::io::Error::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    static PRIORITY: AtomicI32 = AtomicI32::new(-1);
    static POLICY: AtomicI32 = AtomicI32::new(-1);
    static PID: AtomicI32 = AtomicI32::new(-1);

    unsafe extern "C" fn capture(
        pid: libc::pid_t,
        policy: libc::c_int,
        settings: *const libc::sched_param,
    ) -> libc::c_int {
        PID.store(pid, Ordering::SeqCst);
        POLICY.store(policy, Ordering::SeqCst);
        // SAFETY: fifo supplies an initialized stack value, borrowed throughout this call.
        PRIORITY.store(unsafe { (*settings).sched_priority }, Ordering::SeqCst);
        0
    }

    #[test]
    fn initialized_parameter_preserves_source_fifo_selection() {
        fifo(capture).unwrap();
        assert_eq!(PID.load(Ordering::SeqCst), 0);
        assert_eq!(POLICY.load(Ordering::SeqCst), libc::SCHED_FIFO);
        assert_eq!(PRIORITY.load(Ordering::SeqCst), 53);
    }
}
