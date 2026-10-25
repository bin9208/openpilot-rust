fn main() {
    if let Err(error) = openpilot_dev_tools::ui_forms_reference::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
