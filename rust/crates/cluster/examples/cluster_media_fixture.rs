use openpilot_cluster::{media::MediaInput, models::NaviLiveState};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Step {
    now: f64,
    messages: Vec<PathBuf>,
    session: Option<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, path] = args.as_slice() else {
        return Err("cluster_media_fixture CASE".into());
    };
    let steps: Vec<Step> = serde_json::from_slice(&std::fs::read(path)?)?;
    let capacity = openpilot_messaging::services::lookup("carrotNaviMedia")
        .unwrap()
        .queue_size;
    let mut publisher = openpilot_msgq::Publisher::with_capacity("carrotNaviMedia", capacity)?;
    let mut input = MediaInput::isolated()?;
    let mut output = Vec::new();
    for step in steps {
        for path in step.messages {
            publisher.send(&std::fs::read(path)?)?;
        }
        let navi = step.session.map(|session_id| NaviLiveState {
            session_id,
            ..NaviLiveState::default()
        });
        output.push(input.update_at(navi.as_ref(), step.now)?);
    }
    assert!(input.close());
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
