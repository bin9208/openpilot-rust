use std::process::ExitCode;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    match openpilot_dev_tools::manager_cars_generate::cli() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("generate-manager-cars: {error}");
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
    eprintln!("manager car source export requires Linux provider admission");
    ExitCode::FAILURE
}
