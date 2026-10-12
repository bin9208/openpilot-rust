use num_traits::ToPrimitive;
use openpilot_carrot_server::{
    youtube_live::{rtmp::Client, sink::Sink},
    Value,
};
use std::{
    io::{self, BufRead, Write},
    sync::Arc,
};
#[path = "carrot_youtube_transport/monitor.rs"]
mod monitor;
#[path = "carrot_youtube_transport/repeated.rs"]
mod repeated;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    if input.has("repeat_urls") {
        println!("{}", repeated::run(&input)?.encode()?);
        return Ok(());
    }
    let endpoint = input.get("url").string()?;
    let parsed = url::Url::parse(&endpoint)?;
    let host = parsed.host_str().ok_or("fixture host missing")?;
    let address: std::net::IpAddr = host.parse()?;
    if !address.is_loopback() {
        return Err("fixture recipients must be loopback".into());
    }
    let client = Arc::new(Client::new(endpoint)?);
    let mut sink = Sink::new(Arc::clone(&client));
    let started = std::time::Instant::now();
    let mut monitor_observation = None;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        client.connect()?;
        if input.get("pause_after_connect").truth() {
            println!("{{\"ready\":true}}");
            io::stdout().flush()?;
            let mut resume = String::new();
            io::stdin().lock().read_line(&mut resume)?;
        }
        if matches!(input.get("input"), Value::Text(_)) {
            if input.get("monitor_blocked").truth() {
                let (observation, error) = monitor::run(Arc::clone(&client), &mut sink, &input)?;
                monitor_observation = Some(observation);
                if let Some(error) = error {
                    return Err(error.into());
                }
                return Ok(());
            }
            let bytes = std::fs::read(input.get("input").string()?)?;
            let chunk = input.get("chunk").int()?.to_usize().unwrap_or(1).max(1);
            for part in bytes.chunks(chunk) {
                sink.write_all(part)?;
            }
            sink.flush()?;
        }
        Ok(())
    })();
    let connected = client.try_connected()?.map_or(Value::Null, Value::Bool);
    let mut snapshot = Value::object([
        ("ok", Value::Bool(result.is_ok())),
        (
            "error",
            result
                .err()
                .map_or(Value::Null, |error| Value::text(&error.to_string())),
        ),
        ("bytes_written", Value::integer(client.bytes_written())),
        ("connected", connected),
        ("bytes_accepted", Value::integer(sink.bytes_accepted)),
        ("pending_bytes", Value::integer(sink.pending_bytes())),
        ("drain_calls", Value::integer(sink.drain_calls)),
        ("partial_writes", Value::integer(sink.partial_writes)),
        ("seconds", Value::Float(started.elapsed().as_secs_f64())),
    ]);
    if let Some(observation) = monitor_observation {
        if let Value::Object(items) = &mut snapshot {
            items.push(("monitor".chars().map(u32::from).collect(), observation));
        }
    }
    client.close();
    println!("{}", snapshot.encode()?);
    Ok(())
}
