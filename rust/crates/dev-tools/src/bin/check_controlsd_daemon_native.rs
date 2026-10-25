#[path = "../controlsd_daemon_check.rs"]
mod controller;
fn main() -> std::process::ExitCode {
    match controller::cli() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("controlsd daemon native: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
