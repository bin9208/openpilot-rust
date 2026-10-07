use openpilot_control_tools::joystick_input::{self, Entry, Options};
use std::process::ExitCode;

fn main() -> ExitCode {
    match joystick_input::run(Options {
        entry: Entry::Managed,
        input: None,
        frames: None,
    }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("joystick: {error}");
            ExitCode::FAILURE
        }
    }
}
