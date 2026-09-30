use openpilot_startup_ui::{children, native_children::NativeLaunch};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::current_dir()?;
    let binaries = std::env::current_exe()?
        .parent()
        .ok_or("missing executable parent")?
        .parent()
        .ok_or("missing build parent")?
        .to_path_buf();
    let launch = NativeLaunch {
        source_root: root,
        binaries: binaries.clone(),
        launcher: binaries.join("openpilot-process-child"),
    };
    let mut spinner = children::Spinner::new(launch.clone());
    if !spinner.update("Native wrapper actual child update")? {
        return Err("spinner update failed".into());
    }
    std::thread::sleep(std::time::Duration::from_millis(250));
    if !spinner.update_progress(42.0, 100.0)? {
        return Err("spinner progress failed".into());
    }
    spinner.close();
    spinner.close();
    let mut text = children::TextWindow::new(launch, "Native wrapper actual child lifecycle");
    std::thread::sleep(std::time::Duration::from_millis(250));
    if text.status()?.is_some() {
        return Err("text window exited prematurely".into());
    }
    text.close()?;
    text.close()?;
    println!("PASS native wrapper spinner update/progress/kill/reap and text spawn/status/TERM/idempotent close");
    Ok(())
}
