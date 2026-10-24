fn main() {
    if let Err(error) = openpilot_dev_tools::ui_application_lifecycle_raw::build_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
