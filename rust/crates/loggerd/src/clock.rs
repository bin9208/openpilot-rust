use crate::Error;

pub fn now() -> Result<u64, Error> {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    u64::try_from(time.tv_sec)?
        .checked_mul(1_000_000_000)
        .and_then(|seconds| seconds.checked_add(u64::try_from(time.tv_nsec).ok()?))
        .ok_or(Error::Invalid("boot timestamp overflow"))
}
