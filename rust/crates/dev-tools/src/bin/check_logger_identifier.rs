fn main() {
    if let Err(error) = openpilot_dev_tools::logger_identifier_check::cli() {
        eprintln!("Logger identifier check: {error}");
        std::process::exit(1);
    }
}
