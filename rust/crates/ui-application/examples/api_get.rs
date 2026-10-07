use openpilot_ui_application::api::http::Session;
use serde::Deserialize;
use std::time::Duration;
#[derive(Deserialize)]
struct Scene {
    urls: Vec<String>,
    timeout: Option<f64>,
    token: Option<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, input] = args.as_slice() else {
        return Err("api_get INPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut session = Session::new(
        "openpilot-fixture".into(),
        scene.timeout.map(Duration::from_secs_f64),
    );
    let mut output = Vec::new();
    for url in scene.urls {
        output.push(match session.get(&url, scene.token.as_deref()) {
            Ok(response) => serde_json::json!({"status":response.status,"text":response.text}),
            Err(error) => {
                serde_json::json!({"error":error.to_string(),"timeout":error.timed_out()})
            }
        });
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
