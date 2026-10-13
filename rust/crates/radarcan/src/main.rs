use std::process::ExitCode;

fn main() -> ExitCode {
    match openpilot_managed_entry::runtime(
        "openpilot.selfdrive.carrot.radar.radarcan",
        "radarcan",
        || match openpilot_radarcan::native::parse(std::env::args().skip(1))
            .and_then(|arguments| arguments.map_or(Ok(()), openpilot_radarcan::native::run))
        {
            Ok(()) => Ok(()),
            Err(openpilot_radarcan::Error::Signal(signal))
                if signal == signal_hook::consts::SIGINT
                    && std::env::var_os("MANAGER_DAEMON").is_some() =>
            {
                Err(openpilot_managed_entry::StepError::Interrupted)
            }
            Err(openpilot_radarcan::Error::Signal(signal)) => {
                if let Err(error) = signal_hook::low_level::emulate_default_handler(signal) {
                    eprintln!("radarcan signal: {error}");
                }
                Err(openpilot_managed_entry::StepError::raised(
                    openpilot_radarcan::Error::Signal(signal),
                ))
            }
            Err(error) => Err(openpilot_managed_entry::StepError::raised(error)),
        },
    ) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("radarcan: {error}");
            ExitCode::FAILURE
        }
    }
}
