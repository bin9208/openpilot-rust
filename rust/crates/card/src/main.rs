use openpilot_card::runtime::{self, Command, Error};
use std::process::ExitCode;

fn main() -> ExitCode {
    let result = runtime::parse(std::env::args_os().skip(1)).and_then(|command| match command {
        Command::Help => {
            println!("openpilot-card --root CHECKOUT --numerics VERIFIED_DIRECTORY [--max-steps N]\nNative CAN-driven card runtime. --max-steps bounds owned host fixtures.");
            Ok(())
        }
        Command::Run(options) => runtime::run(options),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Interrupted) => ExitCode::from(130),
        Err(error) => {
            eprintln!("card: {error}");
            ExitCode::FAILURE
        }
    }
}
