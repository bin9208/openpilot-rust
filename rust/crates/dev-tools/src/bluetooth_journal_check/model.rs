//! Independent translation of the pinned Python CommandWriter and 721-operation
//! checker schedule. This never links/imports candidate Bluetooth journal code.
use crate::seal::Result;
use openpilot_log_format::JsonValue;
use serde::Serialize;
use serde_json::json;
pub const OPERATIONS: usize = 721;
pub const MACS: [&str; 2] = ["66:C0:0C:7B:6E:71", "AA:BB:CC:DD:EE:FF"];
#[derive(Clone, Serialize)]
#[serde(untagged)]
pub enum Time {
    Integer(u32),
    Float(f64),
}
impl Time {
    fn number(&self) -> f64 {
        match self {
            Self::Integer(n) => f64::from(*n),
            Self::Float(n) => *n,
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    Send {
        address: &'static str,
        action: &'static str,
        at: Time,
        hold: Option<&'static str>,
        repeat: bool,
    },
    Prune {
        addresses: Vec<&'static str>,
        at: Time,
        holds: Option<Vec<&'static str>>,
    },
    Publish {
        channel: &'static str,
    },
}
pub fn operations() -> Vec<Operation> {
    let mut ops = vec![Operation::Publish { channel: "cruise" }];
    for index in 0..300 {
        ops.push(Operation::Send {
            address: MACS[index % 2],
            action: ["accelCruise", "cancel", "laneLeft", "laneRight"][index % 4],
            at: Time::Float(10. + index as f64 * 0.0001),
            hold: None,
            repeat: false,
        });
    }
    ops.extend([
        Operation::Prune {
            addresses: MACS.to_vec(),
            at: Time::Float(10.03),
            holds: None,
        },
        Operation::Prune {
            addresses: vec![MACS[0]],
            at: Time::Float(10.03),
            holds: Some(vec![]),
        },
    ]);
    for (index, at) in [
        0.,
        f64::from_bits(0.4_f64.to_bits() - 1),
        0.4,
        f64::from_bits(0.4_f64.to_bits() + 1),
        10.,
        9.9,
        10.4,
        10.5,
    ]
    .into_iter()
    .enumerate()
    {
        ops.push(Operation::Send {
            address: MACS[0],
            action: "decelCruiseLong",
            at: Time::Float(at),
            hold: Some("/dev/input/event0:up@long"),
            repeat: index % 2 != 0,
        });
        ops.push(Operation::Prune {
            addresses: MACS.to_vec(),
            at: Time::Float(at),
            holds: Some(vec!["/dev/input/event0:up@long"]),
        });
    }
    for index in 0..200 {
        let at = 20. + index as f64 * 0.07;
        ops.push(Operation::Send {
            address: MACS[index % 2],
            action: "gapAdjustCruise",
            at: Time::Float(at),
            hold: [None, Some(""), Some("held-A"), Some("held-B")][index % 4],
            repeat: index % 3 != 0,
        });
        ops.push(Operation::Prune {
            addresses: MACS.to_vec(),
            at: Time::Float(at),
            holds: if index % 3 == 0 {
                None
            } else {
                Some(vec!["held-A"])
            },
        });
    }
    ops.extend([
        Operation::Prune {
            addresses: vec![],
            at: Time::Integer(100),
            holds: Some(vec![]),
        },
        Operation::Publish { channel: "lane" },
    ]);
    ops
}
pub fn payload(operations: &[Operation]) -> Result<Vec<u8>> {
    let mut out = String::new();
    for operation in operations {
        // Generic Python-compatible JSON framing, not candidate journal policy.
        out.push_str(&JsonValue::parse(&serde_json::to_string(operation)?)?.to_json()?);
        out.push('\n');
    }
    Ok(out.into_bytes())
}
#[derive(Clone, Serialize)]
struct Event {
    id: String,
    time: f64,
    action: &'static str,
    address: &'static str,
    hold: Option<&'static str>,
    repeat: bool,
}
struct Source {
    events: [Vec<Event>; 2],
    revisions: [u64; 2],
    sequence: u64,
}
impl Source {
    fn new() -> Self {
        Self {
            events: [vec![], vec![]],
            revisions: [1, 1],
            sequence: 0,
        }
    }
    fn execute(&mut self, op: &Operation) {
        match op {
            Operation::Publish { channel } => self.revisions[usize::from(*channel == "lane")] += 1,
            Operation::Send {
                address,
                action,
                at,
                hold,
                repeat,
            } => {
                let side = usize::from(matches!(*action, "laneLeft" | "laneRight"));
                let now = at.number();
                self.sequence += 1;
                self.events[side].retain(|event| {
                    let age = now - event.time;
                    (0.0..=0.4).contains(&age)
                        && hold.is_none_or(|hold| hold.is_empty() || event.hold != Some(hold))
                });
                self.events[side].push(Event {
                    id: format!("00000000000000000000000000000000:{}", self.sequence),
                    time: now,
                    action,
                    address,
                    hold: *hold,
                    repeat: *repeat,
                });
                let discarded = self.events[side].len().saturating_sub(64);
                if discarded > 0 {
                    drop(self.events[side].drain(..discarded));
                }
                self.revisions[side] += 1;
            }
            Operation::Prune {
                addresses,
                at,
                holds,
            } => {
                for side in 0..2 {
                    let before = self.events[side].len();
                    self.events[side].retain(|event| {
                        let age = at.number() - event.time;
                        addresses.contains(&event.address)
                            && (0.0..=0.4).contains(&age)
                            && event.hold.is_none_or(|hold| {
                                hold.is_empty()
                                    || holds.as_ref().is_none_or(|active| active.contains(&hold))
                            })
                    });
                    if self.events[side].len() != before {
                        self.revisions[side] += 1;
                    }
                }
            }
        }
    }
    fn snapshot(&self) -> Result<JsonValue> {
        let mut files = vec![];
        for (side, channel) in ["cruise", "lane"].into_iter().enumerate() {
            files.push(json!({"channel":channel,"bytes":serde_json::to_string(&json!({"events":self.events[side]}))?,"mode":0o600,"inode":self.revisions[side]}));
        }
        Ok(JsonValue::parse(&serde_json::to_string(&files)?)?)
    }
}
pub fn source_trace(operations: &[Operation]) -> Result<Vec<JsonValue>> {
    let mut source = Source::new();
    operations
        .iter()
        .map(|op| {
            source.execute(op);
            source.snapshot()
        })
        .collect()
}
