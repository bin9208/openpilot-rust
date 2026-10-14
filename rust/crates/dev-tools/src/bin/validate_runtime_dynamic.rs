fn main() {
    if let Err(error) = openpilot_dev_tools::media_validation::cli("dynamic") {
        eprintln!("native dynamic validation: {error}");
        std::process::exit(1);
    }
}
