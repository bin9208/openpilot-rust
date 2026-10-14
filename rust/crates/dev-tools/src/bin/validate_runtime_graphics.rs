fn main() {
    if let Err(error) = openpilot_dev_tools::media_validation::cli("graphics") {
        eprintln!("native graphics validation: {error}");
        std::process::exit(1);
    }
}
