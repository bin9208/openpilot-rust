//! CAN-I/O requests only; shared pack/seed generation stays in protocol QA.
use crate::{journald_check::Random, pandad_protocol_check, seal::Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;
pub fn sent(row: &Value, result: i32) -> Value {
    json!({"offset":row["offset"],"write_result":result,"replies":[],"operations":[{"op":"send","frames":row["frames"]}]})
}
fn reply(data: &[u8], healthy: bool, count: Option<i32>) -> Value {
    json!({"data":data,"healthy":healthy,"count":count.unwrap_or(data.len() as i32)})
}
fn received(chunks: Vec<Value>, offset: u64) -> Value {
    json!({"offset":offset,"write_result":0,"operations":vec![json!({"op":"receive"});chunks.len()],"replies":chunks})
}
pub fn seeds() -> Vec<Value> {
    pandad_protocol_check::seeds()
        .iter()
        .map(|row| sent(row, 0))
        .collect()
}
pub fn packets(encoded: &[Value]) -> Result<Vec<Vec<u8>>> {
    if encoded.len() != 16 {
        return Err("CAN-I/O seed response count differs".into());
    }
    encoded
        .iter()
        .map(|row| {
            let data = row["results"][0]["calls"][0]["data"]
                .as_array()
                .ok_or("CAN-I/O source seed packet missing")?;
            if data.len() < 6 {
                return Err("CAN-I/O seed packet header too short".into());
            }
            data.iter()
                .map(|v| {
                    v.as_u64()
                        .filter(|&n| n <= 255)
                        .map(|n| n as u8)
                        .ok_or_else(|| "CAN-I/O seed byte invalid".into())
                })
                .collect()
        })
        .collect()
}
pub fn cases(packets: &[Vec<u8>], maxout: bool) -> Result<Vec<Value>> {
    if packets.len() != 16 || packets.iter().any(|packet| packet.len() < 6) {
        return Err("CAN-I/O seed packet roster invalid".into());
    }
    let mut rows = pandad_protocol_check::packing()
        .iter()
        .enumerate()
        .map(|(i, row)| sent(row, [-7, 0, 3, 4096][i % 4]))
        .collect::<Vec<_>>();
    for packet in packets {
        let splits = [0, 1, 5, 6, packet.len() - 1, packet.len()]
            .into_iter()
            .collect::<BTreeSet<_>>();
        for split in splits {
            rows.push(received(
                vec![
                    reply(&packet[..split], true, None),
                    reply(&[255; 7], false, None),
                    reply(&packet[split..], true, None),
                ],
                4,
            ));
            rows.push(received(
                vec![
                    reply(&packet[..split], true, None),
                    reply(&[], true, Some(0)),
                    reply(&packet[split..], true, None),
                ],
                8,
            ));
        }
        for bit in 0..8 {
            let mut corrupted = packet.clone();
            corrupted[5] ^= 1 << bit;
            rows.push(received(
                vec![
                    reply(
                        &[packets[0].clone(), corrupted, packets[15].clone()].concat(),
                        true,
                        None,
                    ),
                    reply(packet, true, None),
                ],
                0,
            ));
        }
    }
    let mut rng = Random::new(17503);
    for _ in 0..100 {
        let count = rng.below(19) + 1;
        let stream = (0..count)
            .flat_map(|_| packets[(rng.random() * 16.0) as usize].clone())
            .collect::<Vec<_>>();
        let mut chunks = Vec::new();
        let mut at = 0;
        while at < stream.len() {
            let size = (rng.below(127) + 1) as usize;
            let end = (at + size).min(stream.len());
            chunks.push(reply(&stream[at..end], true, None));
            at = end;
        }
        let offset = [0, 4, 8, (1_u64 << 32) - 1][rng.below(4) as usize];
        rows.push(received(chunks, offset));
    }
    let full = packets[15].repeat(235);
    rows.push(received(
        vec![
            reply(&full[..16384], true, None),
            reply(&full[16384..], true, None),
        ],
        0,
    ));
    for count in [-1, -4, -7] {
        rows.push(received(
            vec![
                reply(&[], false, Some(count)),
                reply(&packets[0], true, None),
            ],
            0,
        ));
        if !maxout {
            rows.push(received(
                vec![
                    reply(&packets[15][..4], true, None),
                    reply(&[], true, Some(count)),
                    reply(&packets[15][4..], true, None),
                ],
                0,
            ));
        }
    }
    Ok(rows)
}
