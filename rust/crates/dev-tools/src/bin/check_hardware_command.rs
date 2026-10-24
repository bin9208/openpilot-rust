fn main() {
    if let Err(error) = openpilot_dev_tools::hardware_command_check::cli() {
        eprintln!("Hardware command fixture: {error}");
        std::process::exit(1);
    }
}
