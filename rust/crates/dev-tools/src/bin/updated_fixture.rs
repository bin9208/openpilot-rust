fn main() {
    if let Err(error) = openpilot_dev_tools::updated_fixture::sudo_cli() {
        eprintln!("Updated fixture overlay command: {error}");
        std::process::exit(1);
    }
}
