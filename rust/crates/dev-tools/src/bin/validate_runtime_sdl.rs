fn main() {
    if let Err(error) = openpilot_dev_tools::media_validation::cli("sdl") {
        eprintln!("native sdl validation: {error}");
        std::process::exit(1);
    }
}
