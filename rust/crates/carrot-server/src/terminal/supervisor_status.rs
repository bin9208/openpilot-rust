//! Preserve the terminal's negative-signal exit-code convention.
pub(super) fn exit_status(status: rustix::process::WaitIdStatus) -> i32 {
    status
        .exit_status()
        .unwrap_or_else(|| -status.terminating_signal().unwrap_or(0))
}
