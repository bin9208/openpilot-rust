fn run() -> Result<(), openpilot_ui_application::Error> {
    let mut root = std::env::current_dir()?;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                println!("Usage: openpilot-ui [--source-root PATH]");
                return Ok(());
            }
            "--source-root" => {
                root = args
                    .next()
                    .ok_or(openpilot_ui_application::Error::Contract(
                        "missing source root",
                    ))?
                    .into()
            }
            _ => {
                return Err(openpilot_ui_application::Error::Contract(
                    "unexpected UI argument",
                ));
            }
        }
    }
    if root.ends_with("openpilot/selfdrive/ui") {
        for _ in 0..3 {
            root.pop();
        }
    }
    let mut runtime = openpilot_ui_application::runtime::Runtime::native(&root)?;
    runtime.run()?;
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            openpilot_startup_ui::logging::emit(
                openpilot_logging::record::Level::Error,
                format!("UI stopped: {error}"),
            );
            eprintln!("UI stopped: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
