fn main() {
    if let Err(error) = openpilot_dev_tools::native_logging_build::cli() {
        eprintln!("native logging source build: {error}");
        std::process::exit(1);
    }
}
