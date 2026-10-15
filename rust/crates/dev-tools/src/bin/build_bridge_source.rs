fn main() {
    if let Err(error) = openpilot_dev_tools::bridge_source_build::cli() {
        eprintln!("bridge source build: {error}");
        std::process::exit(1);
    }
}
