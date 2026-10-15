fn main() {
    if let Err(error) = openpilot_dev_tools::jpeg_sanitizers_check::cli() {
        eprintln!("JPEG sanitizer check: {error}");
        std::process::exit(1);
    }
}
