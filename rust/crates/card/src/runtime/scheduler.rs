use std::{io, path::Path};

type Setter =
    unsafe extern "C" fn(libc::pid_t, libc::c_int, *const libc::sched_param) -> libc::c_int;

#[expect(
    unsafe_code,
    reason = "isolated scheduler ABI; injected calls are checked by Miri"
)]
fn fifo(set: Setter) -> io::Result<()> {
    // SAFETY: libc's GNU and musl sched_param contain only integer fields,
    // so zero initialization establishes every field's validity (UB category 4).
    let mut settings: libc::sched_param = unsafe { std::mem::zeroed() };
    settings.sched_priority = 53;
    // SAFETY: the synchronous C call receives an aligned initialized value
    // for its complete read lifetime; pid 0 selects this thread (category 8).
    if unsafe { set(0, libc::SCHED_FIFO, &settings) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn configure_with(
    tici: bool,
    scheduler: impl FnOnce() -> io::Result<()>,
    affinity: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    if tici {
        scheduler()?;
        affinity()?;
    }
    Ok(())
}

pub fn configure() -> io::Result<()> {
    configure_with(
        Path::new("/TICI").is_file(),
        || fifo(libc::sched_setscheduler),
        || {
            let mut cores = rustix::thread::CpuSet::new();
            cores.set(5);
            Ok(rustix::thread::sched_setaffinity(None, &cores)?)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::RefCell,
        sync::atomic::{AtomicI32, Ordering},
    };
    static PID: AtomicI32 = AtomicI32::new(-1);
    static POLICY: AtomicI32 = AtomicI32::new(-1);
    static PRIORITY: AtomicI32 = AtomicI32::new(-1);

    #[expect(
        unsafe_code,
        reason = "Miri oracle for the production scheduler pointer"
    )]
    unsafe extern "C" fn capture(
        pid: libc::pid_t,
        policy: libc::c_int,
        param: *const libc::sched_param,
    ) -> libc::c_int {
        PID.store(pid, Ordering::Relaxed);
        POLICY.store(policy, Ordering::Relaxed);
        // SAFETY: fifo lends its aligned initialized sched_param during this
        // synchronous callback; no pointer is retained (categories 3 and 6).
        PRIORITY.store(unsafe { (*param).sched_priority }, Ordering::Relaxed);
        0
    }

    #[test]
    fn fifo_passes_source_policy_priority_and_current_thread() {
        fifo(capture).unwrap();
        assert_eq!(PID.load(Ordering::Relaxed), 0);
        assert_eq!(POLICY.load(Ordering::Relaxed), libc::SCHED_FIFO);
        assert_eq!(PRIORITY.load(Ordering::Relaxed), 53);
    }

    #[test]
    fn pc_bypass_and_tici_scheduler_before_affinity_preserve_source_failure() {
        let calls = RefCell::new(Vec::new());
        for tici in [false, true] {
            configure_with(
                tici,
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
        }
        assert_eq!(*calls.borrow(), ["fifo", "affinity"]);
        calls.borrow_mut().clear();
        assert!(configure_with(
            true,
            || Err(io::Error::from_raw_os_error(libc::EPERM)),
            || {
                calls.borrow_mut().push("affinity");
                Ok(())
            }
        )
        .is_err());
        assert!(calls.borrow().is_empty());
        let mut cores = rustix::thread::CpuSet::new();
        cores.set(5);
        assert!(cores.is_set(5));
        assert_eq!(cores.count(), 1);
    }
}
