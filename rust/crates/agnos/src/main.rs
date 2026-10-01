fn main() -> std::process::ExitCode {
    match openpilot_agnos::cli::run() {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::FAILURE,
        Err(error) => {
            eprintln!("agnos: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
