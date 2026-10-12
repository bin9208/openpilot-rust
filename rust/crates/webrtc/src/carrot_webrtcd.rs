fn main() -> std::process::ExitCode {
    match openpilot_managed_entry::runtime(
        "openpilot.system.webrtc.carrot_webrtcd",
        "carrot_webrtcd",
        || {
            openpilot_webrtc::runtime::entrypoint(true)
                .map_err(openpilot_managed_entry::StepError::raised)
        },
    ) {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("carrot_webrtcd: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
