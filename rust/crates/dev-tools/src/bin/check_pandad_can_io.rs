fn main() {
    if let Err(error) = openpilot_dev_tools::pandad_can_io_check::cli() {
        eprintln!("{error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::Interrupted)
            {
                130
            } else {
                1
            },
        );
    }
}
