fn main() -> std::process::ExitCode {
    if let Err(error) = openpilot_startup_ui::app::run(openpilot_startup_ui::app::Kind::Text) {
        eprintln!("text window: {error}");
    }
    std::process::ExitCode::FAILURE
}
