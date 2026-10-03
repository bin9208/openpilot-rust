use openpilot_can::parser::Parser;
use serde_json::{json, Value};
use std::collections::BTreeMap;
pub fn snapshot(p: &Parser) -> Value {
    json!({"updated":[],"checks":[],"bus_timeout":p.bus_timeout(),
        "values":p.states.iter().map(|(id,s)|(*id,&s.values)).collect::<BTreeMap<_,_>>(),
        "all_values":p.states.iter().map(|(id,s)|(*id,&s.all_values)).collect::<BTreeMap<_,_>>(),
        "counters":p.states.iter().map(|(id,s)|(*id,s.counter.to_string())).collect::<BTreeMap<_,_>>(),
        "failures":p.states.iter().map(|(id,s)|(*id,s.counter_fail)).collect::<BTreeMap<_,_>>(),
        "frequencies":p.states.iter().map(|(id,s)|(*id,s.frequency)).collect::<BTreeMap<_,_>>(),
        "thresholds":p.states.iter().map(|(id,s)|(*id,s.timeout_threshold)).collect::<BTreeMap<_,_>>(),
        "timestamps":p.states.iter().map(|(id,s)|(*id,s.timestamps.iter().copied().collect::<Vec<_>>())).collect::<BTreeMap<_,_>>(),
        "raw":p.raw,"seen":p.seen_addresses,"last_nonempty":p.last_nonempty,"last_update":p.last_update,"invalid_count":p.invalid_count})
}
