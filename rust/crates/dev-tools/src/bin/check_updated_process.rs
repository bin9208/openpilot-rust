#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = openpilot_dev_tools::updated_process_check::cli() {
        eprintln!("Updated process check: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Updated process check requires Linux (/proc and pidfd ownership)");
    std::process::exit(1);
}
