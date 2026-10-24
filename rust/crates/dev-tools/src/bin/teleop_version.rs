fn main() {
    if let Err(error) = openpilot_dev_tools::teleop_version::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
