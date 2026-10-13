use openpilot_xiaoge::native::{self, options};
use std::process::ExitCode;

fn main() -> ExitCode {
    match openpilot_managed_entry::runtime(
        "openpilot.selfdrive.carrot.xiaoge_data",
        "xiaoge_data",
        || {
            options::parse(std::env::args_os().skip(1)).and_then(|options| match options {
        None => {
            println!("openpilot-xiaoge --root DIR [--assets DIR] [--config FILE] [--tcp-port PORT] [--http-port PORT] [--frames N] [--device-ip IP]");
            Ok(())
        }
        Some(options) => native::run(options),
      }).map_err(openpilot_managed_entry::StepError::raised)
        },
    ) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xiaoge: {error}");
            ExitCode::FAILURE
        }
    }
}
