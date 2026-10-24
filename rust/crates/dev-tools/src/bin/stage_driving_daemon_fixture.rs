fn main() {
    if let Err(error) = openpilot_dev_tools::driving_fixture_archive::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
