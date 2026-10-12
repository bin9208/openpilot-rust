use openpilot_cluster::renderer::nv12::readback::DirectReadback;
use openpilot_startup_ui::{gles::GlesApi, renderer::TargetInfo};
use serde_json::{json, Value};

#[derive(serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Scenario {
    DirectError,
    DirectSmall,
    DirectInvalid,
    Uncleared,
    AllocationZero,
    AllocationError,
    FenceZero,
    ReadError,
    Timeout,
    WaitError,
    Unexpected,
    MapNull,
    MapCorrupt,
    CopyError,
    CopySmall,
    Success,
    MissingAsync,
}

fn outcome<T: serde::Serialize>(result: Result<T, impl std::fmt::Display>) -> Value {
    match result {
        Ok(value) => json!({"value":value}),
        Err(error) => json!({"error":error.to_string()}),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = std::env::args()
        .nth(1)
        .ok_or("cluster_gles_readback_error_fixture PROVIDER")?;
    let scenario: Scenario = serde_json::from_value(json!(std::env::var("OWNED_GL_SCENARIO")?))?;
    let mut readback = DirectReadback::with_api(GlesApi::open(&provider)?);
    let info = TargetInfo {
        framebuffer: 7,
        texture: 9,
        width: 2,
        height: 2,
    };
    let mut bytes = [0xA5; 23];
    let mut records = vec![
        json!({"supported":readback.async_supported(),"can_enqueue":readback.async_can_enqueue()}),
    ];
    match scenario {
        Scenario::DirectError | Scenario::DirectSmall | Scenario::DirectInvalid => {
            let target = if matches!(scenario, Scenario::DirectInvalid) {
                TargetInfo {
                    framebuffer: 0,
                    ..info
                }
            } else {
                info
            };
            let size = if matches!(scenario, Scenario::DirectSmall) {
                15
            } else {
                23
            };
            records.push(outcome(readback.read_rgba(target, &mut bytes[..size])));
        }
        Scenario::Uncleared
        | Scenario::AllocationZero
        | Scenario::AllocationError
        | Scenario::FenceZero
        | Scenario::ReadError
        | Scenario::Timeout
        | Scenario::WaitError
        | Scenario::Unexpected
        | Scenario::MapNull
        | Scenario::MapCorrupt
        | Scenario::CopyError
        | Scenario::CopySmall
        | Scenario::Success
        | Scenario::MissingAsync => {
            records.push(outcome(readback.enqueue_rgba(info, 100.0)));
            if matches!(scenario, Scenario::Timeout) {
                records.push(outcome(readback.async_ready(101.0)));
                records.push(outcome(readback.async_ready(101.0001)));
            } else if matches!(scenario, Scenario::WaitError | Scenario::Unexpected) {
                records.push(outcome(readback.async_ready(100.0)));
            } else if matches!(
                scenario,
                Scenario::MapNull
                    | Scenario::MapCorrupt
                    | Scenario::CopyError
                    | Scenario::CopySmall
                    | Scenario::Success
            ) {
                let size = if matches!(scenario, Scenario::CopySmall) {
                    15
                } else {
                    23
                };
                records.push(outcome(readback.copy_ready(&mut bytes[..size], 100.0)));
            }
        }
    }
    readback.close()?;
    records.push(json!({"ready":readback.async_ready(100.0)?,"can_enqueue":readback.async_can_enqueue(),"bytes":bytes}));
    println!("{}", serde_json::to_string(&records)?);
    Ok(())
}
