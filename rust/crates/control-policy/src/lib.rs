pub mod drive;
#[cfg(feature = "numerics")]
pub mod flux;
pub mod identity;
pub mod math;
#[cfg(feature = "numerics")]
pub mod nano;
#[cfg(feature = "numerics")]
#[allow(unsafe_code)]
pub mod numerics;
pub mod numpy_exp;
pub mod pid;
pub mod similarity;
pub mod vehicle;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("control policy: {0}")]
    Contract(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[cfg(feature = "numerics")]
    #[error(transparent)]
    Library(#[from] libloading::Error),
}
