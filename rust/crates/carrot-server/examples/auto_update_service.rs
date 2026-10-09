#[path = "auto_update_service/fixture.rs"]
mod fixture;
#[path = "auto_update_service/live.rs"]
mod live;
use fixture::{Fixture, Input};
use openpilot_carrot_server::{
    auto_update::RebootSample,
    auto_update_pull::Notification,
    auto_update_runtime::{notify, Reboot},
    Value,
};
use std::{
    io::{self, BufRead},
    sync::{atomic::Ordering, Arc},
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let input: Input = serde_json::from_str(&line)?;
    let fixture = Fixture::new(&input)?;
    tokio::task::LocalSet::new()
        .run_until(execute(&input, fixture))
        .await
}

async fn execute(input: &Input, fixture: Fixture) -> Result<(), Box<dyn std::error::Error>> {
    match input.mode.as_str() {
        "reboot" => {
            let pull = fixture.pull();
            if let Some(mut reboot) = Reboot::begin(&pull, &input.reboot_mode, &input.head)? {
                for step in &input.steps {
                    let mode = step["mode"].as_str().unwrap_or(&input.reboot_mode);
                    if reboot.select(&pull, mode)? {
                        break;
                    }
                    let gear = Value::text(step["gear"].as_str().unwrap_or("other"));
                    let sample = RebootSample {
                        now: step["now"].as_f64().unwrap_or(0.),
                        selfdrive_valid: step["valid"].as_bool().unwrap_or(false),
                        engaged: step["engaged"].as_bool().unwrap_or(false),
                        car_state_valid: step["car_valid"].as_bool().unwrap_or(false),
                        gear_shifter: &gear,
                        device_state_valid: step["device_valid"].as_bool().unwrap_or(false),
                        device_started: step["started"].as_bool().unwrap_or(true),
                    };
                    if reboot.sample(&pull, &sample, || {
                        Ok(fixture.params.put_bool("DoReboot", true)?)
                    })? {
                        break;
                    }
                }
            }
        }
        "notify" | "clear" => {
            let (_stop, stopped) = tokio::sync::watch::channel(false);
            let context = Notification {
                old_head: input.head.clone(),
                lock: fixture.held(&input.lock)?,
                stopped,
            };
            if input.mode == "notify" {
                notify::Notify::new(
                    Arc::clone(&fixture.service),
                    Some(fixture.params.clone()),
                    input.state.clone(),
                )
                .send(context)
                .await?;
            } else {
                fixture
                    .pull()
                    .clear_recovered_git_ref_error(context)
                    .await?;
            }
        }
        "post" => {
            let payload = Value::parse(&input.steps.first().ok_or("payload missing")?.to_string())?;
            let (ok, status, body) = notify::post_json(&input.head, &payload);
            println!(
                "{}",
                Value::object([
                    ("ok", Value::Bool(ok)),
                    ("status", Value::integer(status)),
                    ("body", Value::text(&body))
                ])
                .encode()?
            );
            return Ok(());
        }
        "manager" => {
            let monitor = openpilot_carrot_server::auto_update_runtime::ManagerMonitor::new(
                fixture.inputs.clone(),
            );
            println!("{{\"ready\":true}}");
            for line in io::stdin().lock().lines() {
                let step: serde_json::Value = serde_json::from_str(&line?)?;
                if step["stop"].as_bool() == Some(true) {
                    break;
                }
                fixture.now.store(
                    step["now"].as_f64().ok_or("clock missing")?.to_bits(),
                    Ordering::SeqCst,
                );
                println!(
                    "{{\"sample\":{},\"creates\":{}}}",
                    monitor.ready(),
                    fixture.creates.load(Ordering::SeqCst)
                );
            }
        }
        "runtime" | "app" | "wait-reboot" => {
            live::run(input, &fixture).await?;
        }
        _ => return Err("unknown mode".into()),
    }
    println!("{}", fixture.result()?.encode()?);
    Ok(())
}
