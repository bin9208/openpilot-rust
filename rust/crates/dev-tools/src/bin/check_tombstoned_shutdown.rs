fn main() {
    if let Err(error) = openpilot_dev_tools::tombstone_shutdown_check::cli() {
        if let Some(openpilot_dev_tools::version_reference_check::Interrupted(signal)) =
            error.downcast_ref()
        {
            let _ = signal_hook::low_level::emulate_default_handler(*signal);
        }
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
