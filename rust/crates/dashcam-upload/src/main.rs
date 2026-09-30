mod driver;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--worker") {
        openpilot_dashcam_upload::worker::run().map_err(Into::into)
    } else {
        driver::run(std::env::current_exe()?)
    }
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dashcam upload: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
