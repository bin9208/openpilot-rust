fn main() {
    if let Err(error) = openpilot_dev_tools::loggerd_check::controllers::pipeline_cli() {
        eprintln!("check-loggerd-log-pipeline: {error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else if let Some(interrupted) =
                error.downcast_ref::<openpilot_dev_tools::loggerd_check::Interrupted>()
            {
                128 + interrupted.0
            } else {
                1
            },
        );
    }
}
