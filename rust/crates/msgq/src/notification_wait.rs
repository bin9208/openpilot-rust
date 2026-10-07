use crate::Error;
use nix::{
    errno::Errno,
    poll::ppoll,
    sys::signal::{SigSet, SigmaskHow, Signal},
};
use std::time::{Duration, Instant};

struct NotificationMask(Option<SigSet>);

impl NotificationMask {
    fn block() -> Result<Self, Error> {
        let mut blocked = SigSet::empty();
        blocked.add(Signal::SIGUSR2);
        let previous = blocked
            .thread_swap_mask(SigmaskHow::SIG_BLOCK)
            .map_err(|error| Error::Io("block msgq notification", error.into()))?;
        Ok(Self(Some(previous)))
    }

    fn restore(mut self) -> Result<(), Error> {
        if let Some(previous) = self.0.take() {
            previous
                .thread_set_mask()
                .map_err(|error| Error::Io("restore msgq notification mask", error.into()))?;
        }
        Ok(())
    }
}

impl Drop for NotificationMask {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            if let Err(error) = previous.thread_set_mask() {
                eprintln!("msgq notification mask restoration failed: {error}");
            }
        }
    }
}

pub(crate) fn poll(
    timeout_ms: i32,
    mut ready: impl FnMut() -> Result<bool, Error>,
) -> Result<(), Error> {
    if timeout_ms < -1 {
        return Err(Error::Invalid("invalid poll timeout"));
    }
    // Blocking before the queue check keeps an intervening SIGUSR2 pending until
    // ppoll atomically restores the caller's mask while entering its wait.
    let mask = NotificationMask::block()?;
    let result = wait(timeout_ms, &mut ready, mask.0);
    mask.restore()?;
    result
}

fn wait(
    timeout_ms: i32,
    ready: &mut impl FnMut() -> Result<bool, Error>,
    original_mask: Option<SigSet>,
) -> Result<(), Error> {
    let deadline = u32::try_from(timeout_ms)
        .ok()
        .map(|milliseconds| Instant::now() + Duration::from_millis(u64::from(milliseconds)));
    if ready()? {
        return Ok(());
    }
    loop {
        let remaining = deadline.map_or(Duration::from_millis(100), |end| {
            end.saturating_duration_since(Instant::now())
        });
        match ppoll(&mut [], Some(remaining.into()), original_mask) {
            Ok(_) | Err(Errno::EINTR) => {}
            Err(error) => return Err(Error::Io("poll msgq", error.into())),
        }
        if ready()? || deadline.is_some_and(|end| Instant::now() >= end) {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_mask_is_restored_after_ready_error_and_unwind() {
        let original = SigSet::thread_get_mask().unwrap();
        let restore = NotificationMask(Some(original));
        let mut caller_mask = original;
        caller_mask.add(Signal::SIGUSR1);
        caller_mask.thread_set_mask().unwrap();

        poll(0, || Ok(true)).unwrap();
        assert_eq!(SigSet::thread_get_mask().unwrap(), caller_mask);
        assert!(poll(0, || Err(Error::Invalid("fixture queue read"))).is_err());
        assert_eq!(SigSet::thread_get_mask().unwrap(), caller_mask);
        let panic = std::panic::catch_unwind(|| poll(0, || panic!("fixture unwind")));
        assert!(panic.is_err());
        assert_eq!(SigSet::thread_get_mask().unwrap(), caller_mask);
        restore.restore().unwrap();
    }

    #[test]
    fn empty_zero_timeout_checks_without_waiting() {
        let started = Instant::now();
        let mut checks = 0;
        poll(0, || {
            checks += 1;
            Ok(false)
        })
        .unwrap();
        assert_eq!(checks, 2);
        assert!(started.elapsed() < Duration::from_millis(100));
    }
}
