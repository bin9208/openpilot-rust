//! Driver turn-stalk interpretation for automatic turn signals. Port of
//! carrot's tesla/carstate.py `decode_tesla_stalk` and the
//! `turnSignalDriver*` / `*BlinkerStalkCount` block.
use std::collections::BTreeMap;

const MAGIC: [u8; 16] = [
    0x9B, 0xE8, 0x2A, 0xD3, 0xD3, 0x83, 0x4C, 0x5E, 0x3F, 0x5E, 0xE2, 0x28, 0x3A, 0x13, 0xAF, 0xCE,
];
/// `CANBUS.party` and `CANBUS.vehicle`.
pub const PARTY: u8 = 0;
pub const VEHICLE: u8 = 1;
pub const ADDRESS: u32 = 0x249;

fn crc8_h2f(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x2f
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// `0` neutral, `1` left, `2` right; `None` for an invalid frame.
pub fn decode(data: &[u8]) -> Option<u8> {
    if data.len() != 4 {
        return None;
    }
    let payload = [data[1] & 0xf0, data[2], data[3], 0];
    let checksum = crc8_h2f(&payload) ^ MAGIC[usize::from(data[1] & 15)];
    let position = data[2] & 15;
    (data[0] == checksum && position < 9).then_some(if position >= 5 {
        1
    } else if position != 0 {
        2
    } else {
        0
    })
}

#[derive(Default)]
struct Input {
    timestamp: u64,
    direction: Option<u8>,
    samples: Vec<(u64, u8)>,
}

#[derive(Default)]
pub struct Stalk {
    inputs: BTreeMap<u8, Input>,
    direction: u8,
    pub counts: [u8; 2],
}

/// What carState publishes.
pub struct Output {
    pub valid: bool,
    pub direction: u8,
    pub counts: [u8; 2],
}

impl Stalk {
    pub fn observe(&mut self, data: &[u8], source: u8, mono: u64) {
        if !matches!(source, PARTY | VEHICLE) {
            return;
        }
        let input = self.inputs.entry(source).or_default();
        input.timestamp = mono;
        input.direction = decode(data);
        match input.direction {
            None => input.samples.clear(),
            Some(direction) => input.samples.push((mono, direction)),
        }
    }

    pub fn update(&mut self, now: u64) -> Output {
        let fresh = |t: u64| now >= t && now - t <= 1_000_000_000;
        let use_vehicle = self.inputs.contains_key(&VEHICLE);
        let source_bus = if use_vehicle { VEHICLE } else { PARTY };
        let samples = |input: &Input| -> Vec<u8> {
            input
                .samples
                .iter()
                .filter(|(t, _)| fresh(*t))
                .map(|(_, d)| *d)
                .collect()
        };
        let mut valid = false;
        let mut direction = 0;
        if let Some(source) = self.inputs.get(&source_bus) {
            let source_samples = samples(source);
            valid = source.timestamp > 0 && fresh(source.timestamp) && source.direction.is_some();
            if use_vehicle {
                if let Some(party) = self.inputs.get(&PARTY).filter(|p| p.timestamp > 0) {
                    if fresh(party.timestamp) {
                        valid &= party.direction.is_some() && party.direction == source.direction;
                        let party_presses: std::collections::BTreeSet<u8> =
                            samples(party).into_iter().filter(|d| *d != 0).collect();
                        let source_presses: std::collections::BTreeSet<u8> =
                            source_samples.iter().copied().filter(|d| *d != 0).collect();
                        if !party_presses.is_empty() && party_presses != source_presses {
                            valid = false;
                        }
                    }
                }
            }
            if valid {
                let sequence = if source_samples.is_empty() {
                    vec![source.direction.unwrap_or(0)]
                } else {
                    source_samples
                };
                for d in sequence {
                    if d != 0 && d != self.direction {
                        let i = usize::from(d - 1);
                        self.counts[i] = self.counts[i].wrapping_add(1);
                    }
                    self.direction = d;
                }
                direction = self.direction;
            }
        }
        for input in self.inputs.values_mut() {
            input.samples.clear();
        }
        Output {
            valid,
            direction,
            counts: self.counts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(position: u8, counter: u8) -> [u8; 4] {
        let mut data = [0, counter & 15, position, 0];
        data[0] =
            crc8_h2f(&[data[1] & 0xf0, data[2], data[3], 0]) ^ MAGIC[usize::from(counter & 15)];
        data
    }

    #[test]
    fn decodes_and_counts_presses() {
        assert_eq!(decode(&frame(0, 1)), Some(0));
        assert_eq!(decode(&frame(6, 2)), Some(1));
        assert_eq!(decode(&frame(2, 3)), Some(2));
        let mut bad = frame(2, 3);
        bad[0] ^= 1;
        assert_eq!(decode(&bad), None);
        let mut stalk = Stalk::default();
        stalk.observe(&frame(0, 1), PARTY, 10);
        assert!(stalk.update(20).valid);
        stalk.observe(&frame(6, 2), PARTY, 30);
        let out = stalk.update(40);
        assert_eq!((out.direction, out.counts), (1, [1, 0]));
    }
}
