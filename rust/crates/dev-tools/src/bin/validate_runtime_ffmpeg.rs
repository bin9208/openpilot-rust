fn main() {
    if let Err(error) = openpilot_dev_tools::media_validation::cli("ffmpeg") {
        eprintln!("native ffmpeg validation: {error}");
        std::process::exit(1);
    }
}
