fn main() {
    if let Err(error) = openpilot_dev_tools::stats_daemon_check::runtime::cli(false) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
