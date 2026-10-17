fn main() {
    if let Err(error) = openpilot_dev_tools::selfdrive_params_check::build_cli() {
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
