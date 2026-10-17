fn main() {
    if let Err(error) = openpilot_dev_tools::panda_runtime_build::cli() {
        eprintln!("native build tool: {error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else {
                1
            },
        );
    }
}
