fn main() {
    if let Err(error) = openpilot_dev_tools::updated_process_check::cli() {
        eprintln!("Updated process check: {error}");
        std::process::exit(1);
    }
}
