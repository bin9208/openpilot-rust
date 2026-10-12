fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() < 3 {
        println!("Usage: openpilot-agnos-updater <updater_path> <manifest_path>");
        return Err("missing updater or manifest path".into());
    }
    let root = std::env::var_os("OPENPILOT_ROOT")
        .or_else(|| std::env::var_os("BASEDIR"))
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let mut runtime = openpilot_ui_application::agnos_updater::runtime::Runtime::native(
        &root,
        std::path::Path::new(&args[1]),
        std::path::Path::new(&args[2]),
    )?;
    runtime.run()?;
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Updater error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
