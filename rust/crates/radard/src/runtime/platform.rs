#![allow(unsafe_code)]
use std::{io, path::Path};
type Setter =
    unsafe extern "C" fn(libc::pid_t, libc::c_int, *const libc::sched_param) -> libc::c_int;
fn fifo(set: Setter) -> io::Result<()> {
    // SAFETY: libc's GNU and musl sched_param contain only integer fields;
    // zero initializes the complete platform layout before the kernel reads it.
    let mut parameters: libc::sched_param = unsafe { std::mem::zeroed() };
    parameters.sched_priority = 51;
    // SAFETY: the synchronous libc call reads this initialized, aligned stack
    // value; pid zero selects the caller and no pointer escapes the call.
    if unsafe { set(0, libc::SCHED_FIFO, &parameters) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub(super) fn configure() -> io::Result<()> {
    if Path::new("/TICI").is_file() {
        fifo(libc::sched_setscheduler)?;
        let mut cores = rustix::thread::CpuSet::new();
        cores.set(5);
        rustix::thread::sched_setaffinity(None, &cores).map_err(io::Error::from)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    static PID: AtomicI32 = AtomicI32::new(-1);
    static POLICY: AtomicI32 = AtomicI32::new(-1);
    static PRIORITY: AtomicI32 = AtomicI32::new(-1);
    unsafe extern "C" fn capture(
        pid: libc::pid_t,
        policy: libc::c_int,
        parameters: *const libc::sched_param,
    ) -> libc::c_int {
        PID.store(pid, Ordering::SeqCst);
        POLICY.store(policy, Ordering::SeqCst);
        // SAFETY: fifo supplies a live aligned sched_param initialized above.
        PRIORITY.store(unsafe { (*parameters).sched_priority }, Ordering::SeqCst);
        0
    }
    #[test]
    fn source_radard_fifo_argument_uses_complete_native_layout() {
        fifo(capture).unwrap();
        assert_eq!(PID.load(Ordering::SeqCst), 0);
        assert_eq!(POLICY.load(Ordering::SeqCst), libc::SCHED_FIFO);
        assert_eq!(PRIORITY.load(Ordering::SeqCst), 51);
    }
}
