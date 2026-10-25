fn main() {
    if let Err(error) = openpilot_dev_tools::pandad_can_io_fixture::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
