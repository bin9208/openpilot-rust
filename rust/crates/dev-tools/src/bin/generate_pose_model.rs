use openpilot_dev_tools::kalman_model_generator::{generate, output_argument, Model};
use std::process::ExitCode;

fn main() -> ExitCode {
    match output_argument(
        std::env::args_os().skip(1),
        "rust/crates/locationd/src/model",
    ) {
        Ok(Some(path)) => match generate(Model::Pose, &path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("pose model generator: {error}");
                ExitCode::FAILURE
            }
        },
        Ok(None) => {
            println!("usage: generate-pose-model [--output DIRECTORY]");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("pose model generator: {error}");
            ExitCode::from(2)
        }
    }
}
