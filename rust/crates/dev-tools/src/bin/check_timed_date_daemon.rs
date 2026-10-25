fn main() {
    if let Err(error) =
        openpilot_dev_tools::timed_live::cli(openpilot_dev_tools::timed_live::Kind::DateDaemon)
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
