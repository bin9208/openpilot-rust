fn main() {
    if let Err(error) = openpilot_dev_tools::ci_receipt_metadata::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
