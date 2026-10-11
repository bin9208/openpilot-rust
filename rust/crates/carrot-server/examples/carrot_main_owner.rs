use openpilot_process_supervision::{
    Execution, ManagedProcess, NativeCommand, ProcessLog, StopOptions,
};
use std::{
    io::{self, BufRead},
    path::PathBuf,
    time::Instant,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    let root = PathBuf::from(&args[1]);
    let endpoint = format!("ipc:///tmp/carrot-main-owner-{}", std::process::id());
    let factory = openpilot_logging::producer::Factory::new(endpoint.clone())?;
    let mut process = ManagedProcess::new(
        "carrot_server".into(),
        Execution::Native(NativeCommand {
            launcher: PathBuf::from(&args[2]),
            basedir: "/data/openpilot".into(),
            cwd: ".".into(),
            argv: vec![
                args[3].clone(),
                "--host".into(),
                "127.0.0.1".into(),
                "--port".into(),
                args[4].clone(),
            ],
        }),
        ProcessLog::new(factory.logger()),
    );
    process.start()?;
    std::fs::write(
        root.join("owner-start.json"),
        serde_json::json!({"pid":process.state()?.pid,"log_endpoint":endpoint}).to_string(),
    )?;
    let line = io::stdin().lock().lines().next().transpose()?;
    if line.as_deref() != Some("stop") {
        return Err("owned stop command missing".into());
    }
    let began = Instant::now();
    let exit = process.stop(StopOptions::default())?;
    std::fs::write(root.join("owner-stop.json"), serde_json::json!({"seconds":began.elapsed().as_secs_f64(),"exit":exit,"has_process":process.has_process()}).to_string())?;
    Ok(())
}
