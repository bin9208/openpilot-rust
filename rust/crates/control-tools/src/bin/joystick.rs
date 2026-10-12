use openpilot_control_tools::joystick_input::{self, Entry, Options};
use std::process::ExitCode;

fn main() -> ExitCode {
    match openpilot_managed_entry::runtime(
        "openpilot.tools.joystick.joystick_control",
        "joystick",
        || {
            joystick_input::run(Options {
                entry: Entry::Managed,
                input: None,
                frames: None,
            })
            .map_err(openpilot_managed_entry::StepError::raised)
        },
    ) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("joystick: {error}");
            ExitCode::FAILURE
        }
    }
}
