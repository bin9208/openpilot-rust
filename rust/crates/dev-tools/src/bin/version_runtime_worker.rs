fn main() {
    if let Err(error) = openpilot_dev_tools::version_collector_check::worker_main() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
