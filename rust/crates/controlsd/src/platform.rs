#![allow(unsafe_code)]
use crate::Error;
use std::path::Path;
type SetScheduler =
    unsafe extern "C" fn(libc::pid_t, libc::c_int, *const libc::sched_param) -> libc::c_int;
fn fifo(set: SetScheduler) -> Result<(), Error> {
    // SAFETY: sched_param contains integer/time fields; zero initializes both GNU and musl layouts.
    let mut settings: libc::sched_param = unsafe { std::mem::zeroed() };
    settings.sched_priority = 53;
    // SAFETY: initialized settings remain valid for this synchronous syscall;
    // pid zero selects this daemon's calling thread before workers are started.
    if unsafe { set(0, libc::SCHED_FIFO, &settings) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
fn configure_with(
    tici: bool,
    fifo: impl FnOnce() -> Result<(), Error>,
    affinity: impl FnOnce() -> Result<(), Error>,
) -> Result<(), Error> {
    if tici {
        fifo()?;
        affinity()?;
    }
    Ok(())
}
pub fn configure() -> Result<(), Error> {
    configure_with(
        Path::new("/TICI").is_file(),
        || fifo(libc::sched_setscheduler),
        || {
            let mut cores = rustix::thread::CpuSet::new();
            cores.set(6);
            rustix::thread::sched_setaffinity(None, &cores).map_err(std::io::Error::from)?;
            Ok(())
        },
    )
}
pub fn monotonic() -> f64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    time.tv_sec as f64 + time.tv_nsec as f64 * 1e-9
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::RefCell,
        sync::atomic::{AtomicI32, Ordering},
    };
    static PRIORITY: AtomicI32 = AtomicI32::new(-1);
    static POLICY: AtomicI32 = AtomicI32::new(-1);
    static PID: AtomicI32 = AtomicI32::new(-1);
    unsafe extern "C" fn capture(
        pid: libc::pid_t,
        policy: libc::c_int,
        settings: *const libc::sched_param,
    ) -> libc::c_int {
        PID.store(pid, Ordering::Relaxed);
        POLICY.store(policy, Ordering::Relaxed);
        // SAFETY: fifo passes its initialized stack sched_param for the call lifetime.
        PRIORITY.store(unsafe { (*settings).sched_priority }, Ordering::Relaxed);
        0
    }
    #[test]
    fn scheduler_ffi_passes_initialized_source_priority() {
        fifo(capture).unwrap();
        assert_eq!(PID.load(Ordering::Relaxed), 0);
        assert_eq!(POLICY.load(Ordering::Relaxed), libc::SCHED_FIFO);
        assert_eq!(PRIORITY.load(Ordering::Relaxed), 53);
    }
    #[test]
    fn pc_bypass_tici_order_and_first_error_are_explicit() {
        let calls = RefCell::new(Vec::new());
        configure_with(
            false,
            || {
                calls.borrow_mut().push("fifo");
                Ok(())
            },
            || {
                calls.borrow_mut().push("affinity");
                Ok(())
            },
        )
        .unwrap();
        assert!(calls.borrow().is_empty());
        configure_with(
            true,
            || {
                calls.borrow_mut().push("fifo");
                Ok(())
            },
            || {
                calls.borrow_mut().push("affinity");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*calls.borrow(), ["fifo", "affinity"]);
        calls.borrow_mut().clear();
        assert!(configure_with(
            true,
            || Err(Error::Contract("scheduler rejected")),
            || {
                calls.borrow_mut().push("affinity");
                Ok(())
            }
        )
        .is_err());
        assert!(calls.borrow().is_empty());
    }
}
