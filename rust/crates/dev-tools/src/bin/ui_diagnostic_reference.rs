fn main() {
    if let Err(error) = openpilot_dev_tools::ui_diagnostic_reference::reference_cli() {
        eprintln!("{error}");
        std::process::exit(1)
    }
}
