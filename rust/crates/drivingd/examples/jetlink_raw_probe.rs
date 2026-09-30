use openpilot_driving_modeld::jetlink;
use openpilot_jetlink::transition::{ControlState, Mode, Outcome, Source, Transition};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let requested = match args.as_slice() {
        [arg] if arg == "raw" => true,
        [arg] if arg == "normal" => false,
        _ => return Err("expected raw or normal".into()),
    };
    let controls = ControlState {
        standstill: true,
        cruise_enabled: false,
        lateral_active: false,
        enabled: false,
    };
    let mut transition = Transition::new(false);
    transition.update(Mode::Shadow, true, true, controls, Outcome::None);
    let selected = transition.update(Mode::ActiveRequest, true, true, controls, Outcome::Valid);
    if selected.source != Source::Jetlink {
        return Err("fixture did not activate".into());
    }
    let raw = jetlink::raw_predictions(requested, selected.source, &[13, 42])?;
    println!(
        "{}",
        serde_json::json!({"source":"jetlink","raw_requested":requested,"raw_present":raw.is_some()})
    );
    Ok(())
}
