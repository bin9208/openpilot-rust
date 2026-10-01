fn main() -> Result<(), openpilot_startup_ui::Error> {
    openpilot_startup_ui::app::run(openpilot_startup_ui::app::Kind::Text)
}
