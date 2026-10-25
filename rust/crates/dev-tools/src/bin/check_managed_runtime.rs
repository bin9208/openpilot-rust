fn main() {
    if let Err(error) = openpilot_dev_tools::managed_runtime_check::cli() {
        eprintln!("{error}");
        let code = if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
            2
        } else if let Some(signal) =
            error.downcast_ref::<openpilot_dev_tools::managed_runtime_check::Interrupted>()
        {
            128 + signal.0
        } else {
            1
        };
        std::process::exit(code);
    }
}
