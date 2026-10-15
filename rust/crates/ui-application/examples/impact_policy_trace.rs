use openpilot_ui_application::impact_prompt::{Policy, Visibility};
use serde::Deserialize;
use std::io::BufRead;
#[derive(Deserialize)]
struct Step {
    token: Option<String>,
    started: bool,
    rebooting: bool,
    touched: bool,
    now: f64,
    alive: bool,
    valid: bool,
    received: f64,
    full: bool,
    critical: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in std::io::stdin().lock().lines() {
        let steps: Vec<Step> = serde_json::from_str(&line?)?;
        let mut policy = Policy::default();
        let output: Vec<_> = steps
            .into_iter()
            .map(|step| {
                policy.notice(step.token.as_deref());
                let consumed = policy.touch(step.started, step.rebooting, step.touched);
                let remaining = policy.present(
                    step.now,
                    Visibility {
                        started: step.started,
                        alive: step.alive,
                        valid: step.valid,
                        received: step.received,
                        full: step.full,
                        critical: step.critical,
                        rebooting: step.rebooting,
                    },
                );
                serde_json::json!({"consumed":consumed,"remaining":remaining,"policy":policy})
            })
            .collect();
        println!("{}", serde_json::to_string(&output)?);
    }
    Ok(())
}
