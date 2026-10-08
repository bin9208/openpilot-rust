pub mod actor;
mod broadcast;
mod bus;
pub mod clock;
pub mod config;
mod diagnostics;
mod dispatch;
mod http;
mod network;
mod parameters;
pub mod time_set;
mod upload;

pub fn run() -> Result<(), crate::Error> {
    actor::run(config::Config::for_runtime()?)
}
