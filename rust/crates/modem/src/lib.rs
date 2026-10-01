//! Native port of system/hardware/tici/modem.py; external PPP/network tools remain native dependencies.
pub mod at;
pub mod config;
pub mod observe;
pub mod ppp;
pub mod runtime;
pub mod state;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Launch(#[from] openpilot_process_supervision::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Contract(&'static str),
}
pub fn monotonic() -> f64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    std::time::Duration::new(
        u64::try_from(time.tv_sec).unwrap_or_default(),
        u32::try_from(time.tv_nsec).unwrap_or_default(),
    )
    .as_secs_f64()
}
