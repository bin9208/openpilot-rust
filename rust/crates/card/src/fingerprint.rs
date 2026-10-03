use openpilot_can::{Frame, Packet};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Legacy {
    pub name: String,
    pub versions: Vec<BTreeMap<u32, usize>>,
}

pub fn catalog() -> Result<Vec<Legacy>, serde_json::Error> {
    serde_json::from_str(include_str!("../data/fingerprints.json"))
}

#[derive(Debug, Serialize)]
pub struct Fingerprint {
    pub observed: Vec<(u8, Vec<(u32, usize)>)>,
    pub selected: Option<String>,
    pub frames: u64,
    pub done: bool,
    legacy: Vec<Legacy>,
    candidates: [Vec<usize>; 2],
}

impl Fingerprint {
    pub fn new(legacy: Vec<Legacy>) -> Self {
        let candidates: Vec<_> = (0..legacy.len()).collect();
        Self {
            observed: (0..8).map(|bus| (bus, Vec::new())).collect(),
            selected: None,
            frames: 0,
            done: false,
            legacy,
            candidates: [candidates.clone(), candidates],
        }
    }

    pub fn observe(&mut self, packets: &[Packet]) {
        // Source processes the entire received batch even if an earlier packet completed fingerprinting.
        for packet in packets {
            for frame in &packet.frames {
                if frame.bus < 128 {
                    let bus = match self.observed.iter().position(|(bus, _)| *bus == frame.bus) {
                        Some(index) => index,
                        None => {
                            self.observed.push((frame.bus, Vec::new()));
                            self.observed.len() - 1
                        }
                    };
                    let signals = &mut self.observed[bus].1;
                    if let Some((_, length)) =
                        signals.iter_mut().find(|(addr, _)| *addr == frame.address)
                    {
                        *length = frame.data.len();
                    } else {
                        signals.push((frame.address, frame.data.len()));
                    }
                }
                for (bus, candidates) in [0, 1].into_iter().zip(&mut self.candidates) {
                    if frame.bus == bus
                        && frame.address < 0x800
                        && !matches!(frame.address, 0x7df | 0x7e0 | 0x7e8)
                    {
                        candidates.retain(|index| compatible(frame, &self.legacy[*index]));
                    }
                }
            }
            for candidates in &self.candidates {
                if candidates.len() == 1 && self.frames > 100 {
                    self.selected = Some(self.legacy[candidates[0]].name.clone());
                }
            }
            let failed = (self.candidates.iter().all(Vec::is_empty) && self.frames > 100)
                || self.frames > 200;
            self.done = failed || self.selected.is_some();
            self.frames = self.frames.saturating_add(1);
        }
    }
}

pub fn compatible(frame: &Frame, legacy: &Legacy) -> bool {
    legacy.versions.iter().any(|version| {
        let length = if frame.address == 1880 {
            Some(8)
        } else {
            version.get(&frame.address).copied()
        };
        length == Some(frame.data.len()) || frame.address >= 0x800
    })
}

pub fn source_repr(observed: &[(u8, Vec<(u32, usize)>)]) -> String {
    let buses: Vec<_> = observed
        .iter()
        .map(|(bus, frames)| {
            let signals: Vec<_> = frames
                .iter()
                .map(|(address, length)| format!("{address}: {length}"))
                .collect();
            format!("{bus}: {{{}}}", signals.join(", "))
        })
        .collect();
    format!("{{{}}}", buses.join(", "))
}
