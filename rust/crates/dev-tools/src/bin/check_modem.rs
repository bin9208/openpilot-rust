use std::process::ExitCode;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    if let Some(status) = openpilot_dev_tools::modem_qa::dispatch_fixture_helper() {
        return ExitCode::from(status.clamp(0, 255) as u8);
    }
    match openpilot_dev_tools::modem_qa_cli::cli() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("modem QA: {error}");
            if error
                .downcast_ref::<openpilot_dev_tools::argparse_scan::ArgumentError>()
                .is_some()
            {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    eprintln!("modem QA requires Linux PTY and process-group semantics");
    ExitCode::FAILURE
}
