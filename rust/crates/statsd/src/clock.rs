use crate::Error;
/// Clock seam also permits deterministic source-oracle runs without a runtime interpreter.
pub trait Clock {
    fn monotonic(&mut self) -> Result<f64, Error>;
    fn timestamp_ns(&mut self) -> Result<i128, Error>;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn monotonic(&mut self) -> Result<f64, Error> {
        let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        monotonic_seconds(time)
    }
    fn timestamp_ns(&mut self) -> Result<i128, Error> {
        let time = rustix::time::clock_gettime(rustix::time::ClockId::Realtime);
        timestamp_ns(time)
    }
}
/// Match datetime.now(UTC).timestamp() * 1e9, including its double rounding.
/// The input is an OS-normalized realtime timespec (0 <= tv_nsec < 1e9).
pub fn timestamp_ns(time: rustix::time::Timespec) -> Result<i128, Error> {
    if !(-62_135_596_800..253_402_300_800).contains(&time.tv_sec) {
        return Err(Error::Configuration("timestamp outside datetime calendar"));
    }
    // Aware datetime.timestamp divides integer total microseconds, not separate
    // seconds plus a fraction. Parsing its exact terminating decimal gives the
    // same correctly rounded double, including dates beyond 2^53 microseconds.
    let micros = i128::from(time.tv_sec) * 1_000_000 + i128::from(time.tv_nsec / 1000);
    let magnitude = micros.unsigned_abs();
    let decimal = format!(
        "{}{}.{:06}",
        if micros < 0 { "-" } else { "" },
        magnitude / 1_000_000,
        magnitude % 1_000_000
    );
    Ok((decimal.parse::<f64>()? * 1e9) as i128)
}

/// CPython _PyTime_AsSecondsDouble keeps integral seconds exact and otherwise
/// converts total nanoseconds to double before division.
pub fn monotonic_seconds(time: rustix::time::Timespec) -> Result<f64, Error> {
    let nanos = i64::try_from(i128::from(time.tv_sec) * 1_000_000_000 + i128::from(time.tv_nsec))
        .map_err(|_| Error::Configuration("monotonic clock exceeds Python time range"))?;
    Ok(if nanos % 1_000_000_000 == 0 {
        (nanos / 1_000_000_000) as f64
    } else {
        nanos as f64 / 1e9
    })
}
