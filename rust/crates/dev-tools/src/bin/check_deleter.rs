fn main() {
    #[cfg(target_os = "linux")]
    if let Err(error) = openpilot_dev_tools::deleter_check::cli() {
        eprintln!("check-deleter: {error}");
        std::process::exit(1);
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("check-deleter requires Linux");
        std::process::exit(1);
    }
}
