fn main() {
    if let Err(error) = openpilot_dev_tools::timed_fixtures::command_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
