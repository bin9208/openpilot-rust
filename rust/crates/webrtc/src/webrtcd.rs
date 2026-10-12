fn main() -> std::process::ExitCode {
    match openpilot_managed_entry::runtime("openpilot.system.webrtc.webrtcd", "webrtcd", || {
        openpilot_webrtc::runtime::entrypoint(false)
            .map_err(openpilot_managed_entry::StepError::raised)
    }) {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("webrtcd: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
