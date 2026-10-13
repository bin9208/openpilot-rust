fn main() -> std::process::ExitCode {
    match openpilot_runtime_launcher::watchdog::main() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
