fn main() {
    if let Err(error) = openpilot_dev_tools::diagnostic_check::cli(
        openpilot_dev_tools::diagnostic_check::Kind::Runtime,
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
