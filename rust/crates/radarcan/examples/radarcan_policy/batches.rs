use openpilot_can::Packet;
use openpilot_radarcan::batch::{Batches, Ego};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Action {
    Can { packets: Vec<Packet> },
    State { state: Ego },
    Take { now: u64 },
}

fn packets(values: impl IntoIterator<Item = Packet>) -> Value {
    Value::Array(
        values
            .into_iter()
            .map(|packet| {
                json!([
                    packet.mono_time,
                    packet
                        .frames
                        .into_iter()
                        .map(|f| { json!([f.address, f.data, f.bus]) })
                        .collect::<Vec<_>>()
                ])
            })
            .collect(),
    )
}

pub fn trace(request: Value) -> Result<Value, serde_json::Error> {
    let actions: Vec<Action> = serde_json::from_value(request["actions"].clone())?;
    let mut state = Batches::default();
    let mut steps = Vec::new();
    for action in actions {
        let result = match action {
            Action::Can { packets } => { state.add_can(packets); Value::Null }
            Action::State { state: ego } => { state.add_state(ego); Value::Null }
            Action::Take { now } => state.take(now).map_or(Value::Null, |value| {
                json!({"ego":value.ego,"packets":packets(value.packets),"error":value.error})
            }),
        };
        steps.push(
            json!({"result":result,"can":packets(state.can.iter().cloned()),
            "states":state.states,"overflowed":state.overflowed}),
        );
    }
    Ok(json!(steps))
}
