use openpilot_bluetooth::{Action, Address, Channel, CommandWriter, Intent, Seconds};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::{self, BufRead},
    os::unix::fs::MetadataExt,
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Operation {
    Send {
        address: String,
        action: Action,
        at: Seconds,
        hold: Option<String>,
        repeat: bool,
    },
    Prune {
        addresses: Vec<String>,
        at: Seconds,
        holds: Option<HashSet<String>>,
    },
    Publish {
        channel: Channel,
    },
}

#[derive(Serialize)]
struct Snapshot {
    channel: Channel,
    bytes: String,
    mode: u32,
    inode: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("runtime directory argument required")?,
    );
    let mut writer = CommandWriter::new(&root);
    for channel in Channel::ALL {
        writer.publish(channel)?;
    }
    for line in io::stdin().lock().lines() {
        let operation: Operation = serde_json::from_str(&line?)?;
        match operation {
            Operation::Send {
                address,
                action,
                at,
                hold,
                repeat,
            } => {
                writer.send(Intent {
                    address: Address::parse(&address)?,
                    action,
                    at,
                    hold,
                    repeat,
                })?;
            }
            Operation::Prune {
                addresses,
                at,
                holds,
            } => {
                let addresses = addresses
                    .iter()
                    .map(|address| Address::parse(address))
                    .collect::<Result<HashSet<_>, _>>()?;
                writer.prune(&addresses, at, holds.as_ref())?;
            }
            Operation::Publish { channel } => writer.publish(channel)?,
        }
        let mut snapshots = Vec::new();
        for channel in Channel::ALL {
            let path = root.join(channel.filename());
            let metadata = fs::metadata(&path)?;
            snapshots.push(Snapshot {
                channel,
                bytes: fs::read_to_string(path)?,
                mode: metadata.mode() & 0o777,
                inode: metadata.ino(),
            });
        }
        println!("{}", serde_json::to_string(&snapshots)?);
    }
    Ok(())
}
