fn main() {
    if let Err(error) = openpilot_dev_tools::checkout_status_reference::cli() {
        eprintln!("checkout reference: {error}");
        std::process::exit(1);
    }
}
