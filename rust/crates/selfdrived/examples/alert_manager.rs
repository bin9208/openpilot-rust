use openpilot_selfdrived::alerts::{Alert, AlertEntry, AlertManager};
use openpilot_selfdrived::state::EventType;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
struct Request {
    frame: i64,
    add: Vec<Alert>,
    clear: Vec<EventType>,
}

#[derive(Serialize)]
struct Response<'a> {
    current: Alert,
    entries: &'a [AlertEntry],
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = AlertManager::default();
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        manager.add_many(request.frame, request.add);
        let current = manager
            .process_alerts(request.frame, &request.clear)
            .clone();
        serde_json::to_writer(
            &mut output,
            &Response {
                current,
                entries: manager.entries(),
            },
        )?;
        writeln!(output)?;
    }
    Ok(())
}
