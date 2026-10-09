//! Active services/vision_test.py runner and terminal command contract.
mod arguments;
mod config;
mod control;
pub mod http;
mod runner;
pub mod status;
mod storage;
use crate::Error;
pub use config::Config;

pub async fn run(config: &Config) -> Result<i32, Error> {
    runner::run(config).await
}
pub async fn run_command(config: &Config, args: &[String]) -> Result<i32, Error> {
    let options = match arguments::parse(args) {
        Ok(options) => options,
        Err(error) => {
            arguments::error(&error);
            return Ok(2);
        }
    };
    match options.action {
        arguments::Action::Start => control::start(config).await,
        arguments::Action::Stop => control::stop(config).await,
        arguments::Action::Status => status::print(config),
        arguments::Action::Logs => {
            println!("[vision_test] log={}", config.log.display());
            for line in storage::tail(config, options.lines) {
                println!("{line}");
            }
            Ok(0)
        }
    }
}
