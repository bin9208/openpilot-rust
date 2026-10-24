fn main() {
    if let Err(error) = openpilot_dev_tools::amplifier_policy_check::cli() {
        eprintln!("Amplifier policy check: {error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else {
                1
            },
        );
    }
}
