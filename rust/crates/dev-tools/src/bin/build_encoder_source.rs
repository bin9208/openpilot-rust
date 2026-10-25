fn main() {
    if let Err(error) = openpilot_dev_tools::encoder_source_build::cli() {
        eprintln!("encoder source build: {error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::encoder_source_build::ArgumentError>() {
                2
            } else {
                1
            },
        );
    }
}
