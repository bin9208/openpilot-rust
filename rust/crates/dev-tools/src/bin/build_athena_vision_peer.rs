fn main() {
    if let Err(error) = openpilot_dev_tools::athena_vision_build::cli() {
        eprintln!("Athena vision peer build: {error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::athena_vision_build::ArgumentError>() {
                2
            } else {
                1
            },
        );
    }
}
