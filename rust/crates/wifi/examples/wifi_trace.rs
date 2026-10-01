use openpilot_wifi::{
    state::State, transition::Signal, ConnectStatus, Event, Network, Snapshot, WifiState,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

#[derive(Deserialize)]
struct Case {
    dongle: Option<String>,
    connections: Vec<(String, String)>,
    actions: Vec<Action>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Action {
    SetState {
        ssid: Option<String>,
        status: ConnectStatus,
    },
    Connect {
        ssid: Option<String>,
    },
    New {
        ssid: String,
        path: String,
    },
    Remove {
        path: String,
    },
    Signal {
        current: u32,
        previous: u32,
        reason: u32,
        connection: Option<String>,
        during: Vec<Option<String>>,
    },
    Initial {
        state: u32,
        connection: Option<String>,
        during: Vec<Option<String>>,
    },
    Networks {
        networks: Vec<Network>,
    },
    Scan {
        active: bool,
        last: f64,
        now: f64,
    },
}
#[derive(Serialize)]
struct Row {
    snapshot: Snapshot,
    events: Vec<Event>,
    epoch: u64,
    lookups: usize,
    saves: Vec<String>,
    active_updates: usize,
    scan_due: Option<bool>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let mut output = Vec::new();
    for case in cases {
        let mut state = State::new(case.dongle.as_deref());
        state.connections = case.connections;
        let (mut lookups, mut active_updates) = (0, 0);
        let mut saves = Vec::new();
        let mut rows = Vec::new();
        for action in case.actions {
            let mut scan_due = None;
            match action {
                Action::SetState { ssid, status } => {
                    state.snapshot.wifi_state = WifiState { ssid, status }
                }
                Action::Connect { ssid } => state.set_connecting(ssid),
                Action::New { ssid, path } => state.new_connection(ssid, path),
                Action::Remove { path } => state.remove_connection(&path),
                Action::Signal {
                    current,
                    previous,
                    reason,
                    connection,
                    during,
                } => {
                    if let Some(pending) = state.begin_transition(Signal {
                        current,
                        previous,
                        reason,
                    }) {
                        lookups += 1;
                        for ssid in during {
                            state.set_connecting(ssid);
                        }
                        let activated = pending.activated;
                        if state.finish_transition(pending, connection.as_deref()) && activated {
                            active_updates += 1;
                            if let Some(path) = connection {
                                saves.push(path);
                            }
                        }
                    }
                }
                Action::Initial {
                    state: device,
                    connection,
                    during,
                } => {
                    let epoch = state.epoch;
                    lookups += 1;
                    for ssid in during {
                        state.set_connecting(ssid);
                    }
                    state.finish_initial_state(epoch, device, connection.as_deref());
                }
                Action::Networks { networks } => state.snapshot.networks = networks,
                Action::Scan { active, last, now } => {
                    state.active = active;
                    state.last_scan = last;
                    scan_due = Some(state.scan_due(now));
                }
            }
            rows.push(Row {
                snapshot: state.snapshot(),
                events: std::mem::take(&mut state.events),
                epoch: state.epoch,
                lookups,
                saves: saves.clone(),
                active_updates,
                scan_due,
            });
        }
        output.push(rows);
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
