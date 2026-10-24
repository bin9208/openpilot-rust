fn main() {
    if let Err(error) = openpilot_dev_tools::stats_daemon_check::interrupted::cli() {
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
