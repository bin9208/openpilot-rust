use std::process::ExitCode;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    match openpilot_dev_tools::manager_qa::cli_manager_compat() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("check-manager: {error}");
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    eprintln!("manager QA requires Linux process and ZMQ IPC semantics");
    ExitCode::FAILURE
}
