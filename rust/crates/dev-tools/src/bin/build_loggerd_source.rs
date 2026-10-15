fn main() {
    if let Err(error) = openpilot_dev_tools::logging_oracle_build::cli("loggerd") {
        eprintln!("loggerd source build: {error}");
        std::process::exit(1);
    }
}
