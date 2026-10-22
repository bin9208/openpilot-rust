fn main() -> std::process::ExitCode {
    match openpilot_dev_tools::control_failures_check::cli() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("control failures: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
