//! Final manager entry latches ordinary worker completion/failure until disabled.
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
pub const FAILURE_MESSAGE: &str = "signalColorShadowDisabled; original control unchanged";
pub fn finish<E, R>(
    outcome: Result<(), E>,
    report: impl FnOnce(E) -> Result<(), R>,
    mut enabled: impl FnMut() -> bool,
    stop: &AtomicBool,
    mut wait: impl FnMut(Duration),
) -> Result<(), R> {
    if let Err(error) = outcome {
        report(error)?;
    }
    while !stop.load(Ordering::Relaxed) && enabled() {
        wait(Duration::from_secs(1));
    }
    Ok(())
}
