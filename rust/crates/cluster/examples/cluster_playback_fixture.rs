use openpilot_cluster::runtime::route::Playback;
use serde::Deserialize;
#[derive(Deserialize)]
struct Case {
    start: f64,
    duration: f64,
    now: f64,
    speed: f64,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    op: String,
    now: f64,
    time: f64,
    sound: bool,
    enabled: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cases: Vec<Case> = serde_json::from_reader(std::io::stdin().lock())?;
    let mut result = Vec::new();
    for case in cases {
        let mut playback = Playback::new(case.start, case.duration, case.now, case.speed);
        let mut rows = Vec::new();
        for step in case.steps {
            let flag = match step.op.as_str() {
                "time" => None,
                "toggle" => {
                    playback.toggle(step.now, step.time);
                    None
                }
                "seek" => {
                    playback.seek(step.now, step.time);
                    None
                }
                "pause" => {
                    playback.pause(step.now, step.time);
                    None
                }
                "finish" => {
                    playback.finish(step.now, case.duration);
                    None
                }
                "retry" => Some(playback.retry_due(step.now)),
                "loop" => {
                    playback.resume_loop(step.now);
                    None
                }
                "cutin" => Some(playback.cutin(step.sound, step.enabled, step.now, step.time)),
                _ => return Err("unknown playback operation".into()),
            };
            rows.push(serde_json::json!({"clock": playback, "time": playback.time(step.now), "flag": flag}));
        }
        result.push(rows);
    }
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
