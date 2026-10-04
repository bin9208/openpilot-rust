use openpilot_radarcan::{reader::Reader, Error};
use serde_json::{json, Value};

pub fn snapshot(reader: Option<&mut Reader>) -> Result<(Value, Vec<String>), Error> {
    let Some(reader) = reader else {
        return Ok((Value::Null, Vec::new()));
    };
    let parser = &mut reader.parser;
    let states = parser
        .states
        .iter()
        .map(|(address, message)| {
            let counter: Value = serde_json::from_str(&message.counter.to_string())?;
            Ok((
                address.to_string(),
                json!({"values":message.values,"all_values":message.all_values,
            "timestamps":message.timestamps,"counter":counter,"counter_fail":message.counter_fail,
            "first_seen":message.first_seen,"last_warning":message.last_warning,
            "frequency":message.frequency,"timeout_threshold":message.timeout_threshold}),
            ))
        })
        .collect::<Result<serde_json::Map<String, Value>, serde_json::Error>>()?;
    let value = json!({"bus":reader.bus,"dbc":parser.dbc.name,"states":states,
        "invalid_count":parser.invalid_count,"last_nonempty":parser.last_nonempty,"last_update":parser.last_update});
    let warnings = parser
        .diagnostics
        .drain(..)
        .map(|warning| warning.message)
        .collect();
    Ok((value, warnings))
}
