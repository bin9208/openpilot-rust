fn main() {
    if let Err(error) = openpilot_dev_tools::ui_render_check::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
