mod clock;
mod discovery;
mod http;
mod options;
mod params;
mod publisher;
mod server;
mod session;
mod shared;
mod socket;
#[cfg(test)]
mod tests;
pub mod wire;

pub use options::Options;
pub use server::run;

use crate::Error;

fn io(error: std::io::Error) -> Error {
    Error::typed("OSError", error.to_string())
}
