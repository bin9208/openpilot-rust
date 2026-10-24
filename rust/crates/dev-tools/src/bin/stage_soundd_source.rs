fn main() {
    if let Err(error) = openpilot_dev_tools::soundd_source_stage::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
