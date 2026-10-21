fn main() {
    if let Err(error) = openpilot_dev_tools::driver_daemon_fixture::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
