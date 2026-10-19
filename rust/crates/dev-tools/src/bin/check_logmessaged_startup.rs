fn main() {
    if let Err(error) = openpilot_dev_tools::logmessaged_startup_check::cli() {
        let code = if openpilot_dev_tools::logmessaged_startup_check::argument_error(error.as_ref())
        {
            2
        } else {
            1
        };
        eprintln!("{error}");
        std::process::exit(code);
    }
}
