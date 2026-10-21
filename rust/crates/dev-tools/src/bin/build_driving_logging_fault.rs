fn main() {
    if let Err(error) = openpilot_dev_tools::driving_logging_fault_build::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
