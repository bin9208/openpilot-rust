use openpilot_dashcam_upload::state::{Clock, Jobs};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, Read};

#[derive(Deserialize)]
struct Command {
    op: String,
    clock: [f64; 2],
    #[serde(default)]
    id: String,
    #[serde(default)]
    segments: Vec<String>,
    text: Option<String>,
    patch: Option<Value>,
    now: Option<f64>,
    #[serde(default)]
    results: Vec<Value>,
    error: Option<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let commands: Vec<Command> = serde_json::from_str(&input)?;
    let mut jobs = Jobs::default();
    let mut observations = Vec::new();
    for command in commands {
        let clock = Clock {
            wall: command.clock[0],
            monotonic: command.clock[1],
        };
        let mut response = Value::Null;
        match command.op.as_str() {
            "create" => jobs.create(command.id, command.segments, clock),
            "expire" => jobs.expire(command.now, clock),
            "cancel" => {
                response = if let Some(job) = jobs.get_mut(&command.id) {
                    job.cancel(clock)?
                } else {
                    json!({"ok":false,"error":"job not found"})
                };
            }
            op => {
                let job = jobs.get_mut(&command.id).ok_or("job missing")?;
                match op {
                    "append" => job.append(command.text.as_deref(), clock),
                    "touch" => job.touch(clock),
                    "progress" => {
                        if let Err(error) = job.update(
                            serde_json::from_value(command.patch.ok_or("patch missing")?)?,
                            clock,
                        ) {
                            response = json!({"error":error.to_string()});
                        }
                    }
                    "finish" => job.finish(
                        serde_json::from_value(command.patch.ok_or("patch missing")?)?,
                        clock,
                    ),
                    "partial" => job.partial_results = command.results,
                    "task_done" => job.task_done = true,
                    "fail" => {
                        job.fail_running(command.error.as_deref().ok_or("error missing")?, clock)
                    }
                    _ => return Err("invalid operation".into()),
                }
                if matches!(op, "finish" | "fail") {
                    jobs.prune()
                }
            }
        }
        observations.push(json!({"response":response,"jobs":jobs.0.iter().map(|job|job.snapshot()).collect::<Result<Vec<_>,_>>()?}));
    }
    println!("{}", serde_json::to_string(&observations)?);
    Ok(())
}
