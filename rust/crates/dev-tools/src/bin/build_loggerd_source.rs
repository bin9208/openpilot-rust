fn main() {
    if let Err(error) = openpilot_dev_tools::logging_oracle_build::cli("loggerd") {
        eprintln!("loggerd source build: {error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else {
                1
            },
        );
    }
}
