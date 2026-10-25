fn main() {
    if let Err(error) = openpilot_dev_tools::bridge_source_build::cli() {
        eprintln!("bridge source build: {error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else {
                1
            },
        );
    }
}
