#[cfg(feature = "native")]
fn main() -> Result<(), openpilot_webcam::Error> {
    let specs = openpilot_webcam::selection::environment(cfg!(target_os = "macos"))?;
    openpilot_webcam::runtime::Camerad::prepare(specs)?.run(|_| Ok(()))?;
    Ok(())
}

#[cfg(not(feature = "native"))]
fn main() {
    eprintln!("webcam runtime requires the native feature");
    std::process::exit(2);
}
