use openpilot_carrot_navi::{
    json::Value,
    manifest::MapConfig,
    packet,
    receiver::{Receiver, Stream},
    record::Clock,
};
use openpilot_cluster::navi_simulator::{Config, Projection, Source};
use serde_json::json;
use std::{
    fs,
    io::{self, BufRead, Write},
};

struct Time {
    wall: i128,
    mono: u128,
}
impl Clock for Time {
    fn wall_ms(&mut self) -> i128 {
        self.wall
    }
    fn mono_ns(&mut self) -> u128 {
        self.mono
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("query")?;
    if input == "--server" {
        return server(args.next().ok_or("port")?.parse()?);
    }
    let query: serde_json::Value = serde_json::from_str(&fs::read_to_string(input)?)?;
    let mut receiver = Receiver::new(
        Value::integer(7714),
        MapConfig {
            theme: "dark".into(),
            ..MapConfig::default()
        },
    );
    let mut projection = Projection::new("127.0.0.1:7714".into(), 10.0);
    let mut results = Vec::new();
    for step in query.as_array().ok_or("steps")? {
        let mut clock = Time {
            wall: step["wall"].as_i64().ok_or("wall")?.into(),
            mono: step["mono"].as_u64().ok_or("mono")?.into(),
        };
        let session = step["session"].as_str().unwrap_or("0011223344556677");
        match step["op"].as_str().ok_or("op")? {
            "negotiate" => {
                receiver.negotiate(
                    &Value::parse(&step["value"].to_string())?,
                    "fixture-app",
                    || Ok(session.into()),
                )?;
            }
            "connect" => receiver.control_connected(),
            "disconnect" => receiver.control_disconnected(),
            "json" => receiver.record_json(
                session,
                step["name"].as_str().ok_or("name")?,
                &Value::parse(&step["value"].to_string())?,
                "fixture-peer",
                &mut clock,
                &mut io::sink(),
            )?,
            "binary" => {
                let bytes = step["bytes"]
                    .as_array()
                    .ok_or("bytes")?
                    .iter()
                    .map(|value| {
                        value
                            .as_u64()
                            .and_then(|v| u8::try_from(v).ok())
                            .ok_or("byte")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let (metadata, payload) = packet::parse(&bytes)?;
                receiver.record_binary(
                    Stream {
                        session,
                        kind: step["kind"].as_str().ok_or("kind")?,
                        name: step["name"].as_str().ok_or("name")?,
                        peer: "fixture-peer",
                    },
                    &metadata.value(),
                    payload,
                    &mut clock,
                )?;
            }
            "update" => {
                let now = step["now"].as_f64().ok_or("now")?;
                let state = projection.update_at(
                    &receiver.dashboard_snapshot(),
                    &receiver.cereal_snapshot(),
                    now,
                    clock.wall,
                    "12:34:56",
                )?;
                results.push(json!({"state": state, "status": projection.status_text()?}));
            }
            _ => return Err("unknown operation".into()),
        }
    }
    projection.close();
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}

fn server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let mut source = Source::new(Config {
        host: "127.0.0.1".into(),
        port,
        beacon: false,
        ..Config::default()
    })?;
    println!("{}", json!({"ready": true, "port": port}));
    io::stdout().flush()?;
    for line in io::stdin().lock().lines() {
        match line?.as_str() {
            "update" => println!(
                "{}",
                json!({"state": source.update()?, "status": source.status_text()?})
            ),
            "close" => {
                source.close()?;
                source.close()?;
                println!("{}", json!({"closed": true}));
                break;
            }
            _ => return Err("unknown server command".into()),
        }
        io::stdout().flush()?;
    }
    Ok(())
}
