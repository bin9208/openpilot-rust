use crate::input::Case;
use openpilot_card::{
    brands::volkswagen::{Error, Volkswagen},
    core::ApplyInput,
};
use openpilot_cereal::car_capnp::{car_control, car_state};
use serde_json::{json, Value};
pub fn logs(vehicle: &mut Volkswagen) -> Value {
    json!(vehicle
        .take_logs()
        .into_iter()
        .map(|l| vec![format!("{:?}", l.level).to_ascii_uppercase(), l.message])
        .collect::<Vec<_>>())
}
fn failure(error: Error) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(match error {
        Error::InheritedMqbNumpy => json!({"kind":"InheritedMqbNumpy","message":error.to_string()}),
        Error::Stock(name) => json!({"kind":"Stock","attribute":name}),
        Error::SourceFunction { module, function } => {
            json!({"kind":"SourceFunction","module":module,"function":function})
        }
        Error::Numeric => json!({"kind":"Numeric"}),
        Error::Can(openpilot_can::Error::Numeric) => json!({"kind":"CanNumeric"}),
        other => return Err(other.into()),
    })
}
pub fn run(
    vehicle: &mut Volkswagen,
    case: &Case,
) -> Result<(Vec<Value>, Option<Value>), Box<dyn std::error::Error>> {
    let mut rows = Vec::new();
    let mut error = None;
    for step in &case.steps {
        let mut state = if matches!(case.op.as_str(), "before_update" | "seeded_controller")
            || case.seed_state.is_some()
        {
            let mut out = openpilot_card::core::Message::new_default();
            out.set_root(
                vehicle
                    .state
                    .out
                    .get_root_as_reader::<car_state::Reader>()?,
            )?;
            out
        } else {
            match vehicle.update(&step.packets, step.now) {
                Ok(out) => out,
                Err(e) if case.op == "mqb_failure" => {
                    error = Some(failure(e)?);
                    break;
                }
                Err(e) => return Err(e.into()),
            }
        };
        vehicle.set_soft_hold(step.soft_hold);
        let mut cs = state.get_root::<car_state::Builder>()?;
        cs.set_v_cruise(step.commit.cruise);
        cs.set_activate_cruise(step.commit.activate);
        if case.op != "before_update" {
            vehicle.commit_state(state.get_root_as_reader()?)?;
        }
        let control = capnp::serialize::read_message(
            std::io::Cursor::new(&step.control),
            capnp::message::ReaderOptions::new(),
        )?;
        let output = match vehicle.apply(ApplyInput {
            control: control.get_root::<car_control::Reader>()?,
            now_ns: step.now,
            model: None,
            radar: None,
        }) {
            Ok(out) => out,
            Err(e) if matches!(case.op.as_str(), "numeric_error" | "before_update") => {
                error = Some(failure(e)?);
                break;
            }
            Err(e) => return Err(e.into()),
        };
        rows.push(json!({"state":capnp::serialize::write_message_to_words(&state),"actuators":capnp::serialize::write_message_to_words(&output.actuators),"can":output.can,"extra":vehicle.state.extras,"state_snapshot":vehicle.state.snapshot(),"controller":vehicle.controller.snapshot(),"packer_counters":vehicle.controller.packer_counters(),"logs":logs(vehicle),"soft_hold":vehicle.state.soft_hold,"is_metric":vehicle.state.is_metric}));
    }
    if matches!(
        case.op.as_str(),
        "mqb_failure" | "before_update" | "numeric_error"
    ) && error.is_none()
    {
        return Err(format!("error fixture unexpectedly succeeded: {}", case.name).into());
    }
    Ok((rows, error))
}
