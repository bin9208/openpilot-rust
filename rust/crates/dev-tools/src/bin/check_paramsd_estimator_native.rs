#[path = "../paramsd_estimator_check.rs"]
mod controller;
fn main() -> std::process::ExitCode {
    match controller::cli() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("paramsd estimator native: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
