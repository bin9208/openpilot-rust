fn main() {
    if let Err(error) = openpilot_dev_tools::logging_oracle_build::cli("bootlog") {
        eprintln!("bootlog source build: {error}");
        std::process::exit(1);
    }
}
