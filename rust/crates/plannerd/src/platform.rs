use crate::Error;
use num_traits::ToPrimitive;
use std::path::Path;

type Scheduler =
    unsafe extern "C" fn(libc::pid_t, libc::c_int, *const libc::sched_param) -> libc::c_int;

fn fifo(set: Scheduler) -> Result<(), Error> {
    // SAFETY: zero initializes every integer field and padding for both GNU and musl sched_param.
    let mut parameters: libc::sched_param = unsafe { std::mem::zeroed() };
    parameters.sched_priority = 51;
    // SAFETY: pid zero selects this calling thread; the initialized stack parameter lives through the syscall.
    if unsafe { set(0, libc::SCHED_FIFO, &parameters) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub fn configure() -> Result<(), Error> {
    if Path::new("/TICI").is_file() {
        fifo(libc::sched_setscheduler)?;
        let mut cores = rustix::thread::CpuSet::new();
        cores.set(4);
        rustix::thread::sched_setaffinity(None, &cores).map_err(std::io::Error::from)?;
    }
    Ok(())
}

pub trait Clock {
    fn monotonic(&self) -> f64;
    fn wall(&self) -> f64;
    fn thread_cpu(&self) -> f64;
    fn message_time(&self) -> Result<u64, Error> {
        (self.monotonic() * 1e9)
            .to_u64()
            .ok_or(Error::Contract("message clock conversion"))
    }
}

pub struct SystemClock;
fn seconds(clock: rustix::time::ClockId) -> f64 {
    let now = rustix::time::clock_gettime(clock);
    now.tv_sec as f64 + now.tv_nsec as f64 * 1e-9
}
impl Clock for SystemClock {
    fn monotonic(&self) -> f64 {
        seconds(rustix::time::ClockId::Monotonic)
    }
    fn wall(&self) -> f64 {
        seconds(rustix::time::ClockId::Realtime)
    }
    fn thread_cpu(&self) -> f64 {
        seconds(rustix::time::ClockId::ThreadCPUTime)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    static OBSERVED: AtomicI32 = AtomicI32::new(0);
    unsafe extern "C" fn scheduler(
        pid: libc::pid_t,
        policy: libc::c_int,
        parameters: *const libc::sched_param,
    ) -> libc::c_int {
        assert_eq!(pid, 0);
        assert_eq!(policy, libc::SCHED_FIFO);
        // SAFETY: fifo passes a live initialized sched_param to this synchronous test boundary.
        OBSERVED.store(unsafe { (*parameters).sched_priority }, Ordering::Relaxed);
        0
    }
    #[test]
    fn native_scheduler_receives_source_planner_priority() {
        fifo(scheduler).unwrap();
        assert_eq!(OBSERVED.load(Ordering::Relaxed), 51);
    }
}
