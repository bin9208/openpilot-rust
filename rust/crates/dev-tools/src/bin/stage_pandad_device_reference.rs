fn main() {
    if let Err(error) = openpilot_dev_tools::pandad_device_fixture::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
