pub mod state;
mod status;
pub use state::{Camera, Metrics, Model, State, Stream};

pub fn rounded(value: f64, places: usize) -> Result<f64, std::num::ParseFloatError> {
    format!("{value:.places$}").parse()
}
