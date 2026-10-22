fn main() {
    if let Err(error) = openpilot_dev_tools::native_logging_check::cli() {
        eprintln!("check-native-logging: {error}");
        std::process::exit(
            if let Some(interrupted) =
                error.downcast_ref::<openpilot_dev_tools::native_logging_check::Interrupted>()
            {
                128 + interrupted.0
            } else if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else {
                1
            },
        );
    }
}
