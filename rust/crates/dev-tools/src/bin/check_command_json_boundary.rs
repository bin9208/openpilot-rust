fn main() {
    if let Err(error) = openpilot_dev_tools::command_reference_check::cli(
        openpilot_dev_tools::command_reference_check::Kind::JsonBoundary,
    ) {
        eprintln!("{error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else {
                1
            },
        );
    }
}
