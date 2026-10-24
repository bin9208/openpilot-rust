fn main() {
    if let Err(error) = openpilot_dev_tools::ui_forms_check::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
