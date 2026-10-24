fn main() {
    if let Err(error) = openpilot_dev_tools::amplifier_linux_check::reference_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
