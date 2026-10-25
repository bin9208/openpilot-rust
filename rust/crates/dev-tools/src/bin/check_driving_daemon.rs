fn main() {
    if let Err(error) = openpilot_dev_tools::driving_daemon_check::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
