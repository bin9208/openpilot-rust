use openpilot_ui_application::onroad::hud::presentation::{self, Navigation, SetSpeed};
use serde::Deserialize;
use std::io::Read;
#[derive(Deserialize)]
struct Step {
    source: String,
    provider: String,
    target: Option<String>,
    desired: Option<String>,
    set_speed: String,
    max_label: String,
    vehicle: bool,
    external: bool,
    owner: String,
    lifecycle: String,
    remote: String,
    connected: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let steps: Vec<Step> = serde_json::from_str(&input)?;
    let mut results = Vec::with_capacity(steps.len());
    for step in steps {
        let value = SetSpeed {
            cruise_target: step.target.as_deref().map(str::parse::<f64>).transpose()?,
            desired_speed: step.desired.as_deref().map(str::parse::<f64>).transpose()?,
            source: &step.source,
            provider: &step.provider,
            set_speed_kph: step.set_speed.parse()?,
            max_label: &step.max_label,
        }
        .compute();
        let (label, mode) = presentation::reason(&step.source, &step.provider);
        let navigation = Navigation {
            vehicle_available: step.vehicle,
            external_active: step.external,
            owner: &step.owner,
            lifecycle: &step.lifecycle,
        }
        .status()
        .map(|(text, color)| (text, color.code()));
        results.push(serde_json::json!({
            "override": {"active": value.active, "speed_bits":value.speed_kph.to_bits().to_string(),
                         "label":value.label,"speed_color_mode":value.speed_color_mode,"force_persist":value.force_persist},
            "reason":(label,mode.code()),"navigation":navigation,
            "connected":presentation::external_connected(&step.remote,step.connected),
        }));
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
