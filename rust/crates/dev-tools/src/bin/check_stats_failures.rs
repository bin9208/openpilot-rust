fn main() {
    if let Err(error) = openpilot_dev_tools::stats_daemon_check::cli("failures") {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
