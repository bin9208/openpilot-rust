#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Contract(&'static str),
    #[error(transparent)]
    Graphics(#[from] openpilot_startup_ui::Error),
    #[error(transparent)]
    Egl(#[from] openpilot_startup_ui::egl::EglError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Cluster(#[from] crate::Error),
    #[error(transparent)]
    Camera(#[from] crate::camera::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
