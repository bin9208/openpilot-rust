use std::process::ExitCode;

fn main() -> ExitCode {
    match openpilot_radarcan::native::parse(std::env::args().skip(1))
        .and_then(|arguments| arguments.map_or(Ok(()), openpilot_radarcan::native::run))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(openpilot_radarcan::Error::Signal(signal)) => {
            if let Err(error) = signal_hook::low_level::emulate_default_handler(signal) {
                eprintln!("radarcan signal: {error}");
            }
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("radarcan: {error}");
            ExitCode::FAILURE
        }
    }
}
