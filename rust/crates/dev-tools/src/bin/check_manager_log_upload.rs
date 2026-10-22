fn main() {
    if let Err(error) = openpilot_dev_tools::route_upload_check::cli(true) {
        eprintln!("check-manager-log-upload: {error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else if let Some(signal) =
                error.downcast_ref::<openpilot_dev_tools::route_upload_check::Interrupted>()
            {
                128 + signal.0
            } else {
                1
            },
        );
    }
}
