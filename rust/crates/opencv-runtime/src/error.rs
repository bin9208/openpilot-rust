#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("OpenCV buffer contract: {0}")]
    Contract(&'static str),
    #[error("OpenCV {operation}: {message}")]
    Native {
        operation: &'static str,
        message: String,
    },
    #[error("OpenCV thread count is already {configured}; requested {requested}")]
    ThreadCount { configured: u32, requested: u32 },
}
