use openpilot_pandad::runtime::run;
use std::{ffi::OsStr, os::unix::ffi::OsStrExt, process::ExitCode};

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let mut frames = None;
    let mut serials = Vec::new();
    while let Some(argument) = args.next() {
        if argument == "--help" {
            println!(
                "openpilot-pandad [--frames N] [SERIAL ...]\n\nRuns native Panda CAN, state, safety and peripheral workers.\nThe firmware wrapper must prepare devices before this daemon starts."
            );
            return ExitCode::SUCCESS;
        }
        if argument == "--frames" && frames.is_none() {
            frames = args
                .next()
                .and_then(|value| value.to_str().and_then(|value| value.parse::<u64>().ok()))
                .filter(|value| *value > 0);
            if frames.is_none() {
                eprintln!("pandad: invalid frame count");
                return ExitCode::FAILURE;
            }
        } else if OsStr::new(&argument).as_bytes().starts_with(b"--") {
            eprintln!("pandad: unknown or duplicate option");
            return ExitCode::FAILURE;
        } else {
            serials.push(argument.as_bytes().to_vec());
        }
    }
    match run(serials, frames) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pandad: {error}");
            ExitCode::FAILURE
        }
    }
}
