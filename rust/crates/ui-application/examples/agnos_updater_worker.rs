use openpilot_ui_application::agnos_updater::worker::{Layout, Options, Worker};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, updater, manifest, launcher, layout, output] = args.as_slice() else {
        return Err("agnos_updater_worker UPDATER MANIFEST LAUNCHER LAYOUT OUTPUT".into());
    };
    let layout = match layout.as_str() {
        "tici" => Layout::Tici,
        "mici" => Layout::Mici,
        _ => return Err("invalid layout".into()),
    };
    let updater = PathBuf::from(updater);
    let mut worker = Worker::new(Options {
        cwd: updater.parent().ok_or("no parent")?.into(),
        updater,
        manifest: manifest.into(),
        launcher: launcher.into(),
        layout,
    });
    worker.start()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let state = loop {
        let state = worker.snapshot()?;
        if !state.running {
            break state;
        }
        if Instant::now() > deadline {
            return Err("owned command did not finish".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    worker.close()?;
    std::fs::write(output, serde_json::to_vec(&state)?)?;
    Ok(())
}
