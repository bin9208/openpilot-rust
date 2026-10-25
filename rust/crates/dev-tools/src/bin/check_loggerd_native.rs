fn main() {
    if let Err(error) = openpilot_dev_tools::loggerd_check::cli() {
        eprintln!("check-loggerd-native: {error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else if let Some(interrupted) =
                error.downcast_ref::<openpilot_dev_tools::loggerd_check::Interrupted>()
            {
                128 + interrupted.0
            } else {
                1
            },
        );
    }
}
