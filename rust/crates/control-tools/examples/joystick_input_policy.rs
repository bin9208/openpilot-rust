use openpilot_control_tools::joystick::{Gamepad, Keyboard, Profile};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Request {
    Keyboard {
        keys: Vec<String>,
    },
    Gamepad {
        profile: Profile,
        events: Vec<Event>,
    },
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Event {
    Input { code: String, state: i32 },
    Error { error: ReadError },
}
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ReadError {
    Unplugged,
    Os,
}

#[derive(Serialize)]
struct Snapshot {
    returned: bool,
    axes: [u64; 2],
    cancel: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    minimum: Option<[u64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maximum: Option<[u64; 2]>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let requests: Vec<Request> = serde_json::from_str(&text)?;
    let mut output = Vec::with_capacity(requests.len());
    for request in requests {
        let rows = match request {
            Request::Keyboard { keys } => {
                let mut owner = Keyboard::default();
                keys.iter()
                    .map(|key| Snapshot {
                        returned: owner.update(key),
                        axes: owner.axes.map(f64::to_bits),
                        cancel: owner.cancel,
                        minimum: None,
                        maximum: None,
                    })
                    .collect::<Vec<_>>()
            }
            Request::Gamepad { profile, events } => {
                let mut owner = Gamepad::new(profile);
                let mut rows = Vec::with_capacity(events.len());
                for event in events {
                    let returned = match event {
                        Event::Input { code, state } => owner.update(&code, state)?,
                        Event::Error { error } => {
                            match error {
                                ReadError::Unplugged | ReadError::Os => owner.disconnected(),
                            };
                            false
                        }
                    };
                    rows.push(Snapshot {
                        returned,
                        axes: owner.axes.map(f64::to_bits),
                        cancel: owner.cancel,
                        minimum: Some(owner.minimum.map(f64::to_bits)),
                        maximum: Some(owner.maximum.map(f64::to_bits)),
                    });
                }
                rows
            }
        };
        output.push(rows);
    }
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &output)?;
    stdout.write_all(b"\n")?;
    Ok(())
}
