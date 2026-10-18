fn main() {
    if let Err(error) = openpilot_dev_tools::tombstone_runtime_check::worker_main() {
        if let Some(openpilot_dev_tools::version_reference_check::Interrupted(signal)) =
            error.downcast_ref()
        {
            let _ = signal_hook::low_level::emulate_default_handler(*signal);
        }
        eprintln!("{error}");
        std::process::exit(1);
    }
}
