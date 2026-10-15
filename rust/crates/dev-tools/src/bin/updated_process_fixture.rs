fn main() {
    if let Err(error) = openpilot_dev_tools::updated_process_fixture::cli() {
        eprintln!("Updated process fixture: {error}");
        std::process::exit(1);
    }
}
