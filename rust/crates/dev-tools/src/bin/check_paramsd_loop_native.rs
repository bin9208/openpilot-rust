fn main() -> std::process::ExitCode {
    match openpilot_dev_tools::paramsd_loop_check::cli() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("paramsd loop comparison: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
