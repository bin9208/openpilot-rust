use openpilot_xiaoge::native::{self, options};
use std::process::ExitCode;

fn main() -> ExitCode {
    match options::parse(std::env::args_os().skip(1)).and_then(|options| match options {
        None => {
            println!("openpilot-xiaoge --root DIR [--assets DIR] [--config FILE] [--tcp-port PORT] [--http-port PORT] [--frames N] [--device-ip IP]");
            Ok(())
        }
        Some(options) => native::run(options),
    }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => { eprintln!("xiaoge: {error}"); ExitCode::FAILURE }
    }
}
