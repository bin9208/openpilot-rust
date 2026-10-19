fn main() {
    if let Err(error) = openpilot_dev_tools::registration_check::cli() {
        eprintln!("{error}");
        std::process::exit(
            if error
                .downcast_ref::<openpilot_dev_tools::argparse_scan::ArgumentError>()
                .is_some()
            {
                2
            } else {
                1
            },
        );
    }
}
