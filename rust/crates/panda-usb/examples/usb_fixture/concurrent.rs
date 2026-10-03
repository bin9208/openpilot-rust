use super::Fixture;
use openpilot_panda_usb::Session;
use serde_json::{json, Value};
use std::{ffi::CStr, sync::Barrier};

pub fn run(
    session: &Session,
    fixture: &Fixture,
    threads: usize,
    transfers: u64,
) -> Result<Value, Box<dyn std::error::Error>> {
    let barrier = Barrier::new(threads + 1);
    // SAFETY: switches this owned fixture into its thread-safe atomic counting mode.
    unsafe { (fixture.concurrent_start)() };
    let errors = std::thread::scope(|scope| {
        let workers = (0..threads)
            .map(|_| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    (0..transfers)
                        .filter(|_| !matches!(session.control_write(0xdc, 1, 2, 5), Ok(0)))
                        .count()
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        workers
            .into_iter()
            .map(|worker| worker.join().map_err(|_| "USB fixture thread panicked"))
            .sum::<Result<usize, _>>()
    })?;
    // SAFETY: every transfer thread joined; the fixture returns stable owned JSON.
    let metrics: Value = serde_json::from_slice(
        unsafe { CStr::from_ptr((fixture.concurrent_result)()) }.to_bytes(),
    )?;
    Ok(
        json!({"metrics": metrics, "errors": errors, "connected": session.connected(), "healthy": session.healthy()}),
    )
}
