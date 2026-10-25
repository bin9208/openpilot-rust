fn main() {
    if let Err(error) = openpilot_dev_tools::ui_application_logic_check::cli(
        openpilot_dev_tools::ui_application_logic_check::Kind::Stack,
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
