fn main() {
    if let Err(error) = openpilot_dev_tools::stats_clock_check::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
