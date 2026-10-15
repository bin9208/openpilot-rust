use openpilot_card::runtime::{self, Command, Error};
use std::process::ExitCode;

fn main() -> ExitCode {
    let result = openpilot_managed_entry::runtime("openpilot.selfdrive.car.card", "card", || {
        match runtime::parse(std::env::args_os().skip(1)).and_then(|command| match command {
        Command::Help => {
            println!("openpilot-card --root CHECKOUT --numerics VERIFIED_DIRECTORY [--dbc DIRECTORY] [--max-steps N] [--frequency-trace PATH] [--fixture-phase-fence PATH]\nNative CAN-driven card runtime. The phase fence is only available with bounded fixture frequency diagnostics.");
            Ok(())
        }
        Command::Run(options) => runtime::run(options),
        }) {
            Ok(()) => Ok(()),
            Err(Error::Interrupted) => Err(openpilot_managed_entry::StepError::Interrupted),
            Err(error) => Err(openpilot_managed_entry::StepError::raised(error)),
        }
    });
    match result {
        Ok(openpilot_managed_entry::Outcome::Interrupted)
            if std::env::var_os("MANAGER_DAEMON").is_none() =>
        {
            ExitCode::from(130)
        }
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("card: {error}");
            ExitCode::FAILURE
        }
    }
}
