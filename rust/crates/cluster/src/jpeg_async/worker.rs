use super::{state::Shared, Output};
use std::{
    io,
    sync::mpsc::SyncSender,
    time::{Duration, Instant},
};
fn sample(shared: &Shared, name: &str, start: Instant) -> io::Result<()> {
    shared
        .lock()
        .map_err(io::Error::other)?
        .samples
        .push((name.into(), start.elapsed().as_secs_f64() * 1000.0));
    Ok(())
}
fn frames(output: &mut impl Output, shared: &Shared) -> io::Result<()> {
    loop {
        let mut state = shared.lock().map_err(io::Error::other)?;
        while state.pending.is_none() && state.controls.is_empty() && !state.closing {
            state = shared
                .ready
                .wait_timeout(state, Duration::from_millis(100))
                .map_err(|_| io::Error::other("JPEG worker state poisoned"))?
                .0;
        }
        if let Some((control, reply)) = state.controls.pop_front() {
            drop(state);
            let _ = reply.send(output.control(control));
            continue;
        }
        let Some(frame) = state.pending.take() else {
            return Ok(());
        };
        shared.ready.notify_all();
        drop(state);
        output.clear_profile_samples();
        let start = Instant::now();
        let jpeg = output.encode_jpeg(&frame)?;
        sample(shared, "usb_async.encode_jpeg", start)?;
        let start = Instant::now();
        output.send_jpeg(&jpeg)?;
        sample(shared, "usb_async.send_jpeg", start)?;
        shared
            .lock()
            .map_err(io::Error::other)?
            .samples
            .extend(output.profile_samples());
        output.clear_profile_samples();
    }
}
pub(super) fn run<S: Output + 'static>(
    factory: impl FnOnce() -> io::Result<S>,
    shared: &Shared,
    ready: &SyncSender<io::Result<Option<[u32; 2]>>>,
) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut output = match factory() {
            Ok(output) => output,
            Err(error) => {
                let _ = ready.send(Err(error));
                return Ok(());
            }
        };
        let _ = ready.send(Ok(output.dimensions()));
        frames(&mut output, shared)
    }));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => shared.failure(error),
        Err(_) => shared.failure(io::Error::other("JPEG worker panicked")),
    }
    shared.finish();
}
