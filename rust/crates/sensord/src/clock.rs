use crate::Clock;
#[derive(Clone, Copy)]
pub struct SystemClock;
impl Clock for SystemClock {
    fn monotonic(&mut self) -> f64 {
        let t = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        t.tv_sec as f64 + t.tv_nsec as f64 / 1e9
    }
    fn monotonic_ns(&mut self) -> i128 {
        let t = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        i128::from(t.tv_sec) * 1_000_000_000 + i128::from(t.tv_nsec)
    }
    fn realtime_ns(&mut self) -> i128 {
        let t = rustix::time::clock_gettime(rustix::time::ClockId::Realtime);
        i128::from(t.tv_sec) * 1_000_000_000 + i128::from(t.tv_nsec)
    }
    fn sleep(&mut self, seconds: f64) {
        if seconds > 0. {
            std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
        }
    }
}
pub fn message_time(clock: &mut impl Clock) -> Result<u64, crate::Error> {
    let ns = (clock.monotonic() * 1e9).trunc();
    if !(0.0..18446744073709551616.0).contains(&ns) {
        return Err(crate::Error::Contract("message timestamp out of range"));
    }
    Ok(ns as u64)
}
#[derive(Default)]
pub struct Ratekeeper {
    next: Option<f64>,
}
impl Ratekeeper {
    pub fn keep_time(&mut self, clock: &mut impl Clock) {
        if self.next.is_none() {
            self.next = Some(clock.monotonic() + 0.5);
            let _ = clock.monotonic();
        }
        let _ = clock.monotonic();
        if let Some(next) = self.next {
            let remaining = next - clock.monotonic();
            self.next = Some(next + 0.5);
            if remaining > 0. {
                clock.sleep(remaining);
            }
        }
    }
}
