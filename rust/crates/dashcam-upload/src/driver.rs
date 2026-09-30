use openpilot_dashcam_upload::{manager::Manager, worker::Settings};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};
#[derive(Deserialize)]
struct Request {
    op: String,
    #[serde(default)]
    id: String,
    root: Option<PathBuf>,
    #[serde(default)]
    segments: Vec<String>,
    settings: Option<Settings>,
    now: Option<f64>,
}
pub fn run(executable: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = Manager::new(executable);
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        let result = match request.op.as_str() {
            "start" => manager.start(
                request
                    .root
                    .as_deref()
                    .unwrap_or(std::path::Path::new("/data/media/0/realdata")),
                &request.segments,
                request.settings,
            ),
            "snapshot" => manager.snapshot(&request.id),
            "cancel" => manager.cancel(&request.id),
            "expire" => manager.expire(request.now).map(|()| json!({"ok":true})),
            "jobs" => manager.jobs().and_then(|jobs| {
                Ok(Value::Array(
                    jobs.iter()
                        .map(|job| job.snapshot())
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            }),
            _ => return Err("invalid operation".into()),
        };
        let result = result.unwrap_or_else(|error| json!({"ok":false,"error":error.to_string()}));
        println!("{}", serde_json::to_string(&result)?);
        io::stdout().flush()?;
    }
    Ok(())
}
