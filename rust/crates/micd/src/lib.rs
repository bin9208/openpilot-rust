pub mod analysis;
pub mod runtime;
mod wire;
pub const SAMPLE_RATE: u32 = 16_000;
pub const SAMPLE_BUFFER: usize = 800;
pub const FFT_SAMPLES: usize = 1600;
pub const RATE: u32 = 10;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Audio(#[from] openpilot_portaudio::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error("{0}")]
    Contract(&'static str),
}
