fn main() -> std::process::ExitCode {
    match openpilot_runtime_launcher::bootstrap::main() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("native startup: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
