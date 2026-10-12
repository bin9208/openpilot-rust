fn main() -> std::process::ExitCode {
    match openpilot_cluster::runtime::command_line(false) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
