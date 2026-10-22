fn main() {
    if let Err(error) = openpilot_dev_tools::uploader_lifecycle_check::cli() {
        eprintln!("check-uploader-siblings: {error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else {
                1
            },
        );
    }
}
