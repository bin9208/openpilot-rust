use super::{platform, Error, Shared};
use crate::spi_alert::Tracker;
use openpilot_logging::record::Level;
use openpilot_params::Params;
use std::{sync::atomic::Ordering, time::Duration};

pub fn run(shared: &Shared) -> Result<(), Error> {
    platform::thread_name(c"pandad_spi_diag")?;
    let params = Params::for_runtime()?;
    let diagnostics = shared.factory.spi_diagnostics();
    let mut observed = diagnostics.error_sequence();
    let mut pending = false;
    let mut tracker = Tracker::default();
    while !shared.stop.load(Ordering::Relaxed) {
        let now_ms = platform::now_ns() / 1_000_000;
        let current = diagnostics.error_sequence();
        let onroad = shared.onroad.load(Ordering::Relaxed);
        tracker.update_onroad(onroad, now_ms);
        if !onroad {
            observed = current;
            pending = false;
        } else {
            if current != observed {
                let count = current.wrapping_sub(observed);
                let event = diagnostics.error_event();
                observed = current;
                if tracker.observe(now_ms, count, event.final_result < 0) {
                    pending = true;
                    shared.logs.write(Level::Warning, format!(concat!(
                        "spi_tmux_candidate_diag: sequence={}, events={}, endpoint=0x{:x}, first_result={}, final_result={}",
                        ", attempts={}, recoveries={}, tx_len={}, max_rx_len={}, timeout_ms={}",
                        ", phase={}, lock_us={}, turnaround_us={}, hack_us={}, dack_us={}, recovery_us={}, total_us={}, recovery_restarts={}"),
                        event.sequence, count, event.endpoint, event.result, event.final_result,
                        event.attempts, event.recoveries, event.tx_len, event.max_rx_len, event.timeout_ms,
                        event.phase, event.lock_us, event.turnaround_us, event.hack_us, event.dack_us,
                        event.recovery_us, event.total_us, event.recovery_restarts));
                }
            }
            if pending && tracker.ready(now_ms) {
                let reason = params.get("CarrotException")?.unwrap_or_default();
                if reason.is_empty() {
                    params.put("CarrotException", b"spi_error")?;
                    shared.logs.write(
                        Level::Warning,
                        "spi_tmux_trigger: queued CarrotException=spi_error",
                    );
                    tracker.mark_capture_requested();
                    pending = false;
                } else if reason == b"spi_error" {
                    shared.logs.write(
                        Level::Warning,
                        "spi_tmux_trigger: coalesced with CarrotException=spi_error",
                    );
                    tracker.mark_capture_requested();
                    pending = false;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
