use std::{io, path::Path};

type Setter =
    unsafe extern "C" fn(libc::pid_t, libc::c_int, *const libc::sched_param) -> libc::c_int;

fn fifo(set: Setter) -> io::Result<()> {
    // SAFETY: GNU and musl sched_param contain only integer fields. A complete
    // zeroed value is valid and remains aligned for the synchronous setter call.
    let mut parameter: libc::sched_param = unsafe { std::mem::zeroed() };
    parameter.sched_priority = 51;
    // SAFETY: the setter reads the initialized local value only during this call.
    if unsafe { set(0, libc::SCHED_FIFO, &parameter) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn configure() -> io::Result<()> {
    if Path::new("/TICI").is_file() {
        fifo(libc::sched_setscheduler)?;
        let mut cores = rustix::thread::CpuSet::new();
        cores.set(4);
        rustix::thread::sched_setaffinity(None, &cores).map_err(io::Error::from)?;
    }
    Ok(())
}

#[expect(
    clippy::expect_used,
    reason = "Linux monotonic clock is nonnegative and within the wire u64 nanosecond range"
)]
pub fn monotonic_ns() -> u64 {
    let value = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    u64::try_from(i128::from(value.tv_sec) * 1_000_000_000 + i128::from(value.tv_nsec))
        .expect("monotonic timestamp fits wire u64")
}

pub fn seconds(id: rustix::time::ClockId) -> f64 {
    let value = rustix::time::clock_gettime(id);
    value.tv_sec as f64 + value.tv_nsec as f64 / 1e9
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn capture(
        pid: libc::pid_t,
        policy: libc::c_int,
        parameter: *const libc::sched_param,
    ) -> libc::c_int {
        assert_eq!((pid, policy), (0, libc::SCHED_FIFO));
        // SAFETY: fifo supplies an aligned, fully initialized local sched_param.
        assert_eq!(unsafe { (*parameter).sched_priority }, 51);
        0
    }
    #[test]
    fn scheduler_argument_abi_preserves_source_fifo_priority() {
        fifo(capture).unwrap();
    }
}
