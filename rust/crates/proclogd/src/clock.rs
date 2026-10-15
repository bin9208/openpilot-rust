use openpilot_runtime_core::procfs::SamplingClock;

pub struct Clock;
fn nanoseconds(id: rustix::time::ClockId) -> u64 {
    let now = rustix::time::clock_gettime(id);
    u64::try_from(now.tv_sec)
        .expect("nonnegative Linux clock")
        .checked_mul(1_000_000_000)
        .and_then(|seconds| seconds.checked_add(u64::try_from(now.tv_nsec).ok()?))
        .expect("Linux monotonic clock fits nanoseconds")
}
impl SamplingClock for Clock {
    fn monotonic_ns(&self) -> u64 {
        nanoseconds(rustix::time::ClockId::Monotonic)
    }
    fn thread_cpu_ns(&self) -> u64 {
        nanoseconds(rustix::time::ClockId::ThreadCPUTime)
    }
}
