fn main() {
    if let Err(error) = openpilot_dev_tools::timed_live::worker_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
