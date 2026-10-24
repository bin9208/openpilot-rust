fn main() {
    if let Err(error) =
        openpilot_dev_tools::timed_check::cli(openpilot_dev_tools::timed_check::Kind::Reference)
    {
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
