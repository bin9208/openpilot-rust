#[path = "../controlsd_check.rs"]
mod controller;
fn main() -> std::process::ExitCode {
    match controller::cli() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("controlsd native: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
