//! Request-only transcription of pandad_protocol_cases.py; no codec responses.
use crate::{journald_check::Random, seal::Result};
use serde_json::{json, Value};
pub const LENGTHS: [usize; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];
const ADDRESSES: [u64; 7] = [0, 1, 0x7ff, 0x800, 0x1fffffff, 0x20000000, 0xffffffff];
pub const INVALID: [usize; 17] = [
    9, 10, 11, 13, 15, 17, 19, 21, 23, 25, 31, 33, 47, 49, 63, 65, 255,
];
fn frame(address: u64, source: u64, size: usize, rng: &mut Random) -> Value {
    json!({"address":address,"src":source,"data":(0..size).map(|_|rng.below(256)).collect::<Vec<_>>()})
}
pub fn seeds() -> Vec<Value> {
    let mut rng = Random::new(175);
    LENGTHS.iter().enumerate().map(|(i,&size)|json!({"op":"pack","offset":0,"frames":[frame(ADDRESSES[i%7],0,size,&mut rng)]})).collect()
}
pub fn packing() -> Vec<Value> {
    let mut rng = Random::new(175);
    let mut result = Vec::new();
    for offset in [
        0_u64,
        4,
        252,
        253,
        255,
        256,
        (1_u64 << 32) - 4,
        (1_u64 << 32) - 1,
    ] {
        for size in LENGTHS {
            let frames = (0..256)
                .map(|source| frame(ADDRESSES[source as usize % 7], source, size, &mut rng))
                .collect::<Vec<_>>();
            result.push(json!({"op":"pack","offset":offset,"frames":frames}));
        }
    }
    for count in [0, 1, 3, 31, 32, 33, 64, 255, 1024] {
        for size in [0, 8, 12, 64] {
            let frames = (0..count)
                .map(|index| frame(index, index % 4, size, &mut rng))
                .collect::<Vec<_>>();
            result.push(json!({"op":"pack","offset":0,"frames":frames}));
        }
    }
    for _ in 0..500 {
        let offset = [0, 4, 8, 252][rng.below(4) as usize];
        let count = rng.below(80);
        let frames = (0..count)
            .map(|_| {
                let address = rng.below(1_u64 << 32);
                let source = rng.below(256);
                let size = LENGTHS[rng.below(16) as usize];
                frame(address, source, size, &mut rng)
            })
            .collect::<Vec<_>>();
        result.push(json!({"op":"pack","offset":offset,"frames":frames}));
    }
    result
}
pub fn decoding(encoded: &[Value]) -> Result<Vec<Value>> {
    if encoded.len() != 16 {
        return Err("protocol seed response count differs".into());
    }
    let packets = encoded
        .iter()
        .map(|entry| {
            let chunks = entry["chunks"]
                .as_array()
                .ok_or("seed chunks are not an array")?;
            let packet = chunks
                .first()
                .and_then(Value::as_array)
                .ok_or("seed first chunk missing")?;
            packet
                .iter()
                .map(|value| {
                    value
                        .as_u64()
                        .filter(|&n| n <= 255)
                        .map(|n| n as u8)
                        .ok_or_else(|| "seed byte invalid".into())
                })
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    if packets.iter().any(|packet| packet.len() < 6) {
        return Err("protocol seed header too short".into());
    }
    let mut result = Vec::new();
    for packet in &packets {
        for bus in 0..8_u8 {
            for flags in 0..4_u8 {
                let mut raw = packet.clone();
                raw[0] = (raw[0] & 0xf1) | (bus << 1);
                raw[1] = (raw[1] & 0xfc) | flags;
                raw[5] = 0;
                raw[5] = raw.iter().fold(0, |checksum, byte| checksum ^ byte);
                for offset in [0_u64, 4, (1_u64 << 32) - 1] {
                    let split = (usize::from(bus) * 4 + usize::from(flags)) % (raw.len() + 1);
                    result.push(json!({"op":"decode","offset":offset,"chunks":[[],raw[..split],[],raw[split..]]}));
                }
            }
        }
    }
    let stream = packets.concat();
    for size in [
        1, 2, 3, 4, 5, 6, 7, 8, 16, 63, 64, 65, 70, 255, 256, 257, 16384,
    ] {
        result.push(
            json!({"op":"decode","offset":4,"chunks":stream.chunks(size).collect::<Vec<_>>()}),
        );
    }
    for checksum in 0..256_u16 {
        let mut broken = packets[8].clone();
        broken[5] = checksum as u8;
        result.push(json!({"op":"decode","offset":0,"chunks":[[packets[0].clone(),broken,packets[15].clone()].concat(),packets[3]]}));
    }
    for split in 0..=packets[15].len() {
        result.push(
            json!({"op":"decode","offset":8,"chunks":[packets[15][..split],packets[15][split..]]}),
        );
    }
    let mut rng = Random::new(517);
    for _ in 0..500 {
        let count = rng.below(24) + 1;
        // random.choices uses random() rather than integer rejection sampling.
        let mut selected = (0..count)
            .flat_map(|_| packets[(rng.random() * 16.0) as usize].clone())
            .collect::<Vec<_>>();
        if rng.below(2) != 0 {
            let index = rng.below(selected.len() as u64) as usize;
            selected[index] ^= 1 << rng.below(8);
        }
        let mut chunks = Vec::new();
        let mut at = 0;
        while at < selected.len() {
            let size = (rng.below(127) + 1) as usize;
            let end = (at + size).min(selected.len());
            chunks.push(selected[at..end].to_vec());
            at = end;
        }
        let offset = [0, 4, 8][rng.below(3) as usize];
        result.push(json!({"op":"decode","offset":offset,"chunks":chunks}));
    }
    Ok(result)
}
pub fn alerts() -> Vec<Value> {
    let mut result = Vec::new();
    for start in [0_u64, 1, u64::MAX - 1000] {
        for count in [0_u64, 1, 2, 3, u64::MAX] {
            for terminal in [false, true] {
                let mut operations =
                    vec![json!({"op":"onroad","now":start.to_string(),"value":true})];
                let mut now = String::new();
                for elapsed in [4999, 5000, 5001, 5999, 6000, 10000, 10001, 15000, 15001] {
                    now = start.wrapping_add(elapsed).to_string();
                    operations.push(json!({"op":"observe","now":now,"count":count.to_string(),"terminal":terminal}));
                    operations.push(json!({"op":"ready","now":now}));
                }
                operations.extend([
                    json!({"op":"mark","now":now}),
                    json!({"op":"observe","now":now,"count":"3","terminal":true}),
                    json!({"op":"ready","now":now}),
                    json!({"op":"onroad","now":now,"value":false}),
                    json!({"op":"ready","now":now}),
                ]);
                result.push(json!({"op":"alerts","operations":operations}));
            }
        }
    }
    let mut rng = Random::new(751);
    for _ in 0..100 {
        let mut now = [0, u64::MAX - 100][rng.below(2) as usize];
        let mut operations = Vec::new();
        for _ in 0..300 {
            now = now.wrapping_add(
                [0, 1, 999, 1000, 4999, 5000, 9999, 10000, 10001][rng.below(9) as usize],
            );
            let draw = rng.random() * 24.0;
            let op = if draw < 3.0 {
                "onroad"
            } else if draw < 15.0 {
                "observe"
            } else if draw < 23.0 {
                "ready"
            } else {
                "mark"
            };
            let mut value = json!({"op":op,"now":now.to_string()});
            if op == "onroad" {
                value["value"] = json!(rng.below(4) != 0);
            } else if op == "observe" {
                value["count"] =
                    json!([0_u64, 1, 2, 3, u64::MAX][rng.below(5) as usize].to_string());
                value["terminal"] = json!(rng.below(4) == 0);
            }
            operations.push(value);
        }
        result.push(json!({"op":"alerts","operations":operations}));
    }
    result
}
