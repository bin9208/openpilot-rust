use openpilot_bluetooth::{
    bluez::{Action, Bluez},
    Address,
};
use openpilot_logmessaged::JsonValue;
use std::{error::Error, io::Write};
use tokio::io::{AsyncBufReadExt, BufReader};

enum Command {
    Snapshot,
    Scan,
    Device { address: String, action: Action },
    Pair { address: String },
    Cancel,
    Respond { id: String, value: JsonValue },
    Close,
    Reset,
}

impl Command {
    fn parse(line: &str) -> Result<Self, Box<dyn Error>> {
        let request = JsonValue::parse(line)?;
        let string = |name: &str| -> Result<String, Box<dyn Error>> {
            request
                .get(name)
                .and_then(|value| value.to_utf8())
                .ok_or_else(|| format!("{name} must be a string").into())
        };
        Ok(match string("op")?.as_str() {
            "snapshot" => Self::Snapshot,
            "scan" => Self::Scan,
            "device" => Self::Device {
                address: string("address")?,
                action: serde_json::from_str(
                    &request.get("action").ok_or("action required")?.to_json()?,
                )?,
            },
            "pair" => Self::Pair {
                address: string("address")?,
            },
            "cancel" => Self::Cancel,
            "respond" => Self::Respond {
                id: string("id")?,
                value: request.get("value").ok_or("value required")?,
            },
            "close" => Self::Close,
            "reset" => Self::Reset,
            _ => return Err("unknown command".into()),
        })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let bus = std::env::args()
        .nth(1)
        .ok_or("private bus address required")?;
    let mut client = Bluez::new(Some(bus));
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let command = Command::parse(&line)?;
        let closed = matches!(command, Command::Close);
        let result: Result<serde_json::Value, Box<dyn Error>> = async {
            match command {
                Command::Snapshot => return Ok(serde_json::to_value(client.snapshot().await?)?),
                Command::Scan => client.scan().await?,
                Command::Device { address, action } => {
                    client
                        .device_action(&Address::parse(&address)?, action)
                        .await?
                }
                Command::Pair { address } => client.start_pair(&Address::parse(&address)?).await?,
                Command::Cancel => client.cancel_pair().await?,
                Command::Respond { id, value } => client.respond(&id, value)?,
                Command::Close | Command::Reset => client.close().await?,
            }
            Ok(serde_json::Value::Null)
        }
        .await;
        let response = match result {
            Ok(value) => serde_json::json!({"result":value}),
            Err(error) => serde_json::json!({"error":error.to_string()}),
        };
        println!("{response}");
        std::io::stdout().flush()?;
        if closed {
            return Ok(());
        }
    }
    client.close().await?;
    Ok(())
}
