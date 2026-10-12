#[cfg(feature = "native")]
fn run() -> Result<(), openpilot_webcam::Error> {
    let specs = openpilot_webcam::selection::environment(cfg!(target_os = "macos"))?;
    openpilot_webcam::runtime::Camerad::prepare(specs)?.run(|_| Ok(()))?;
    Ok(())
}

#[cfg(feature = "native")]
fn main() -> std::process::ExitCode {
    match openpilot_managed_entry::runtime("openpilot.tools.webcam.camerad", "webcamerad", || {
        run().map_err(openpilot_managed_entry::StepError::raised)
    }) {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("webcamerad: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(feature = "native"))]
fn main() {
    eprintln!("webcam runtime requires the native feature");
    std::process::exit(2);
}
