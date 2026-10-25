fn main() {
    if let Err(error) = openpilot_dev_tools::ui_adapter_sanitizers::cli(
        openpilot_dev_tools::ui_adapter_sanitizers::Kind::Model,
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
