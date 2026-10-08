use openpilot_cereal::log_capnp::onroad_event::EventName;
use openpilot_selfdrived::alerts::Alert;
use openpilot_selfdrived::events::{Catalog, Events};
use openpilot_selfdrived::state::EventType;
use serde::Deserialize;
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
struct Request {
    reset: Option<bool>,
    clear: bool,
    add: Vec<(u16, bool)>,
    categories: Vec<EventType>,
    callback_alert: Alert,
    prefix: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut events = Events::new(Catalog::load(false)?);
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        if let Some(mici) = request.reset {
            events = Events::new(Catalog::load(mici)?);
        }
        if request.clear {
            events.clear();
        }
        for (event, static_event) in request.add {
            events.add(EventName::try_from(event)?, static_event);
        }
        let mut callbacks = Vec::new();
        let alerts = events.create_alerts(
            &request.categories,
            |callback| {
                callbacks.push(serde_json::to_value(callback)?);
                Ok::<_, serde_json::Error>(request.callback_alert.clone())
            },
            |text| {
                if text.is_empty() {
                    String::new()
                } else {
                    format!("{}{text}", request.prefix)
                }
            },
        )?;
        let names: Vec<u16> = events.names().iter().map(|name| (*name).into()).collect();
        let messages: Vec<_> = events.names().iter().map(|event| serde_json::json!({
            "name": u16::from(*event), "categories": events.categories(*event).map(|entry|entry.category).collect::<Vec<_>>()
        })).collect();
        let contains: Vec<_> = request
            .categories
            .iter()
            .map(|category| events.contains(*category))
            .collect();
        serde_json::to_writer(
            &mut output,
            &serde_json::json!({
                "names":names,"counters":events.counters(),"messages":messages,"contains":contains,"alerts":alerts,"callbacks":callbacks
            }),
        )?;
        writeln!(output)?;
    }
    Ok(())
}
