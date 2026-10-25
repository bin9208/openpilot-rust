fn main() {
    if let Err(error) = openpilot_dev_tools::ui_updater_handshake::cli() {
        eprintln!("UI updater handshake: {error}");
        std::process::exit(1);
    }
}
