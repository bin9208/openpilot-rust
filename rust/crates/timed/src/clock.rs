use crate::Error;
use chrono::{DateTime, Local, NaiveDateTime};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub trait Clock {
    fn wall_nanos(&self) -> Result<u64, Error>;
    fn monotonic(&self) -> Result<u64, Error>;
    fn local(&self, epoch: f64) -> Result<NaiveDateTime, Error>;
    fn sleep(&self, duration: Duration, stop: &AtomicBool);
    fn wall_seconds(&self) -> Result<f64, Error> {
        Ok(Duration::from_nanos(self.wall_nanos()?).as_secs_f64())
    }
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn wall_nanos(&self) -> Result<u64, Error> {
        u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Error::Contract("wall clock before epoch"))?
                .as_nanos(),
        )
        .map_err(|_| Error::Contract("wall clock overflow"))
    }
    fn monotonic(&self) -> Result<u64, Error> {
        let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        let seconds =
            u64::try_from(time.tv_sec).map_err(|_| Error::Contract("negative monotonic clock"))?;
        let nanos =
            u64::try_from(time.tv_nsec).map_err(|_| Error::Contract("negative nanoseconds"))?;
        seconds
            .checked_mul(1_000_000_000)
            .and_then(|v| v.checked_add(nanos))
            .ok_or(Error::Contract("monotonic overflow"))
    }
    fn local(&self, epoch: f64) -> Result<NaiveDateTime, Error> {
        Ok(datetime(epoch)?.with_timezone(&Local).naive_local())
    }
    fn sleep(&self, duration: Duration, stop: &AtomicBool) {
        let deadline = Instant::now() + duration;
        while !stop.load(Ordering::Relaxed) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            std::thread::sleep(remaining.min(Duration::from_millis(20)));
        }
    }
}
/// Python datetime.fromtimestamp rounds to the nearest microsecond, ties to even.
pub fn datetime(epoch: f64) -> Result<DateTime<chrono::Utc>, Error> {
    if !epoch.is_finite() {
        return Err(Error::Contract("nonfinite timestamp"));
    }
    let seconds = epoch.floor();
    let seconds: i64 = format!("{seconds:.0}")
        .parse()
        .map_err(|_| Error::Contract("timestamp out of range"))?;
    let micros: i64 = format!("{:.0}", ((epoch - epoch.floor()) * 1e6).round_ties_even())
        .parse()
        .map_err(|_| Error::Contract("microsecond out of range"))?;
    DateTime::from_timestamp(seconds, 0)
        .and_then(|v| v.checked_add_signed(chrono::Duration::microseconds(micros)))
        .ok_or(Error::Contract("timestamp out of range"))
}
pub fn bounds(clock: &dyn Clock, systemd: &Path) -> Result<(NaiveDateTime, NaiveDateTime), Error> {
    let minimum = datetime(1740096000.0)?.naive_utc(); // 2025-02-21, interpreted as a naive local date.
    let maximum = datetime(2051222400.0)?.naive_utc(); // 2035-01-01.
    let minimum = if systemd.exists() {
        let modified = systemd.metadata()?.modified()?;
        let epoch = match modified.duration_since(UNIX_EPOCH) {
            Ok(value) => value.as_secs_f64(),
            Err(error) => -error.duration().as_secs_f64(),
        };
        let local = clock
            .local(epoch)?
            .checked_add_signed(chrono::Duration::days(1))
            .ok_or(Error::Contract("systemd date overflow"))?;
        minimum.max(local)
    } else {
        minimum
    };
    Ok((minimum, maximum))
}
pub fn valid(clock: &dyn Clock, systemd: &Path) -> Result<bool, Error> {
    let (minimum, maximum) = bounds(clock, systemd)?;
    let now = clock.local(clock.wall_seconds()?)?;
    Ok(minimum < now && now < maximum)
}
