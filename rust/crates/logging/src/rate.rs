//! Callsite-owned `cloudlog_rl` state; timestamps are CLOCK_BOOTTIME nanoseconds.
use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    /// Emit this warning count before the admitted current message, at the same callsite.
    pub suppressed: u32,
    pub emit: bool,
}

pub struct RateLimit {
    burst: u32,
    period_ns: u64,
    begin: u64,
    printed: u32,
    missed: u32,
}
impl Default for RateLimit {
    fn default() -> Self {
        Self::new(2, 100)
    }
}
impl RateLimit {
    /// Supports the source macro's nonnegative burst/millisecond configurations.
    pub const fn new(burst: u32, millis: u32) -> Self {
        Self {
            burst,
            period_ns: millis as u64 * 1_000_000,
            begin: 0,
            printed: 0,
            missed: 0,
        }
    }
    pub fn admit(&mut self, timestamp_ns: u64) -> Result<Decision, Error> {
        if self.begin == 0 {
            self.begin = timestamp_ns;
        }
        let mut suppressed = 0;
        // Source uses unsigned addition, a strict boundary, and restarts on the NEXT call.
        if self.begin.wrapping_add(self.period_ns) < timestamp_ns {
            suppressed = self.missed;
            self.begin = 0;
            self.printed = 0;
            self.missed = 0;
        }
        let emit = self.printed < self.burst;
        let counter = if emit {
            &mut self.printed
        } else {
            &mut self.missed
        };
        if *counter == i32::MAX as u32 {
            return Err(Error::CountOverflow);
        }
        *counter += 1;
        Ok(Decision { suppressed, emit })
    }
}
