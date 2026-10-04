use openpilot_can::{parser::Parser, Packet};
use openpilot_radarcan::{databases, integer_set::IntegerSet};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Request {
    dbc_name: String,
    dbc: String,
    messages: Vec<(u32, f64)>,
    bus: u8,
    constructor_ns: u64,
    actions: Vec<Action>,
}
#[derive(Deserialize)]
struct Action {
    packets: Vec<Packet>,
    #[serde(default)]
    clear: bool,
}

pub fn trace(request: Value, stdout: &mut String) -> Result<Value, Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_value(request)?;
    let dbc = databases::parse(&request.dbc_name, &request.dbc, &mut |line| {
        stdout.push_str(line)
    })?;
    let mut parser = Parser::new(dbc, request.bus, request.constructor_ns);
    for (address, frequency) in request.messages {
        parser.add_address(address, Some(frequency), false, request.constructor_ns)?;
    }
    let mut accumulated = IntegerSet::default();
    let mut output = Vec::new();
    for action in request.actions {
        if action.clear {
            accumulated.clear();
        }
        parser.update(&action.packets)?;
        let updated = IntegerSet::from_arrivals(parser.successful_addresses().iter().copied());
        accumulated.merge(&updated);
        let valid = parser.can_valid();
        let states = parser
            .states
            .iter()
            .map(|(address, state)| {
                let counter: Value = serde_json::from_str(&state.counter.to_string())?;
                Ok((
                    address.to_string(),
                    json!({"values":state.values,"all_values":state.all_values,
                "timestamps":state.timestamps,"counter":counter,"counter_fail":state.counter_fail,
                "first_seen":state.first_seen,"last_warning":state.last_warning}),
                ))
            })
            .collect::<Result<serde_json::Map<String, Value>, serde_json::Error>>()?;
        let warnings = parser
            .diagnostics
            .iter()
            .map(|warning| warning.message.clone())
            .collect::<Vec<_>>();
        output.push(json!({"arrivals":parser.successful_addresses(),"updated":updated.iter().collect::<Vec<_>>(),
            "accumulated":accumulated.iter().collect::<Vec<_>>(),"states":states,"valid":valid,
            "invalid_count":parser.invalid_count,"last_nonempty":parser.last_nonempty,
            "last_update":parser.last_update,"warnings":warnings}));
        parser.diagnostics.clear();
    }
    Ok(json!(output))
}
