fn main() {
    if let Err(error) = openpilot_dev_tools::upstream_sync::cli() {
        eprintln!("Upstream sync: {error}");
        std::process::exit(1);
    }
}
