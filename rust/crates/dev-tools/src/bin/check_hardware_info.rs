use std::process::ExitCode;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    match openpilot_dev_tools::hardware_info_qa::cli() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("HardwareInfo QA: {error}");
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
    eprintln!("HardwareInfo QA requires Linux Unix socket semantics");
    ExitCode::FAILURE
}
