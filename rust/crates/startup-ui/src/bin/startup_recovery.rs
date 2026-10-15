fn main() -> std::process::ExitCode {
    match openpilot_startup_ui::recovery::app::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("startup recovery: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
