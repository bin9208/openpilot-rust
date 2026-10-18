fn main() {
    if let Err(error) = openpilot_dev_tools::version_collector_check::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
