use std::process::ExitCode;
fn main() -> ExitCode {
    #[cfg(target_os = "linux")]
    match openpilot_dev_tools::hardwared_cli::daemon_cli() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("hardwared daemon QA: {error}");
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("hardwared daemon QA requires Linux");
        ExitCode::FAILURE
    }
}
