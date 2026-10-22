fn main() {
    if let Err(error) = openpilot_dev_tools::native_logging_check::cli() {
        eprintln!("check-native-logging: {error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else {
                1
            },
        );
    }
}
