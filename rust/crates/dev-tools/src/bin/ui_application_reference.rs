fn main() {
    if let Err(error) = openpilot_dev_tools::ui_application_lifecycle_reference::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
