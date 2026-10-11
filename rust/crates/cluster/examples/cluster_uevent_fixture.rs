use openpilot_cluster::autorun::events::Events;
use std::{
    io::{self, Read},
    os::unix::net::UnixDatagram,
    time::Duration,
};

#[derive(serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Case {
    Factory,
    Wait {
        packets: Vec<Vec<u8>>,
        timeouts_ms: Vec<u64>,
        expected: u16,
        absent: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let case: Case = serde_json::from_str(&input)?;
    let output = match case {
        Case::Factory => serde_json::json!(Events::open().available()),
        Case::Wait {
            packets,
            timeouts_ms,
            expected,
            absent,
        } => {
            let (receiver, sender) = UnixDatagram::pair()?;
            receiver.set_nonblocking(true)?;
            for packet in packets {
                sender.send(&packet)?;
            }
            let events = if absent {
                Events::default()
            } else {
                Events::from_socket(receiver.into())
            };
            serde_json::json!(timeouts_ms
                .into_iter()
                .map(|timeout| events.wait(Duration::from_millis(timeout), expected))
                .collect::<Vec<_>>())
        }
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
