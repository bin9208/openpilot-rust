use openpilot_control_tools::{
    joystick_input::{self, Entry, Options},
    Error,
};
use std::{env, process::ExitCode};

fn run() -> Result<(), Error> {
    let mut options = Options {
        entry: Entry::GamepadCli,
        input: None,
        frames: None,
    };
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("openpilot-joystick-control [--keyboard] [--frames N]\nPublishes original joystick axes at 100 Hz. Requires IsOffroad unless ZMQ is present.\n--frames bounds host verification publications.\nSIGINT/SIGTERM restore the keyboard terminal and stop the process.");
                return Ok(());
            }
            "--keyboard" => options.entry = Entry::KeyboardCli,
            "--frames" if options.frames.is_none() => {
                options.frames = Some(
                    args.next()
                        .ok_or(Error::Contract("missing frame count"))?
                        .parse()
                        .map_err(|_| Error::Contract("frame count must be positive"))?,
                )
            }
            _ => return Err(Error::Contract("unknown or duplicate argument; see --help")),
        }
    }
    joystick_input::run(options)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("joystick: {error}");
            ExitCode::FAILURE
        }
    }
}
