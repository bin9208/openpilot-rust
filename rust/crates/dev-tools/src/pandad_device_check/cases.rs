//! Dynamic transcription of pandad_device_cases.py; no response corpus.
use crate::journald_check::Random; // Shared CPython MT19937 mechanics, no journal policy.
use serde_json::{json, Value};

pub fn cases() -> Vec<Value> {
    let mut output = Vec::new();
    for hardware in 0..256 {
        output.push(
            json!({"replies":[{"request":0xc1,"count":1,"bytes":[hardware]}],"operations":[]}),
        );
    }
    let mut rng = Random::new(175);
    for (request, operation, size) in [
        (0xd2, "health", 58),
        (0xc2, "can_health", 64),
        (0xb2, "fan_speed", 2),
        (0xd0, "serial", 16),
    ] {
        for count in [-4_i32, -1].into_iter().chain(0..=size) {
            for pattern in [0, 255, 73] {
                let payload = (0..count.max(0))
                    .map(|_| {
                        if pattern == 73 {
                            rng.below(256)
                        } else {
                            pattern
                        }
                    })
                    .collect::<Vec<_>>();
                let mut op = json!({"op":operation});
                if operation == "can_health" {
                    op["bus"] = json!([0, 1, 2, 3, 65535][rng.below(5) as usize]);
                }
                output.push(json!({"replies":[{"request":0xc1,"count":0,"bytes":[]},{"request":request,"count":count,"bytes":payload}],"operations":[op]}));
            }
        }
    }
    for _ in 0..1000 {
        let health = (0..58).map(|_| rng.below(256)).collect::<Vec<_>>();
        let can = (0..64).map(|_| rng.below(256)).collect::<Vec<_>>();
        let bus = rng.below(65536);
        output.push(json!({"replies":[{"request":0xc1,"count":-4,"bytes":[]},{"request":0xd2,"count":58,"bytes":health},{"request":0xc2,"count":64,"bytes":can}],"operations":[{"op":"health"},{"op":"can_health","bus":bus}]}));
    }
    for first in [-4_i32, 0, 63, 64] {
        for second in [-4_i32, 0, 63, 64] {
            output.push(json!({"replies":[{"request":0xc1,"count":1,"bytes":[10]},{"request":0xd3,"count":first,"bytes":vec![1;first.max(0) as usize]},{"request":0xd4,"count":second,"bytes":vec![2;second.max(0) as usize]}],"operations":[{"op":"signature"}]}));
        }
    }
    for length in [0, 1, 2, 16, 63, 64] {
        for terminal in [-4, -1, 0] {
            let mut data = (0..length).map(|_| rng.below(256)).collect::<Vec<_>>();
            if length != 0 {
                data[length / 2] = 0;
            }
            let mut replies = vec![json!({"request":0xc1,"count":1,"bytes":[7]})];
            if length != 0 {
                for _ in 0..3 {
                    replies.push(json!({"request":0xe0,"count":length,"bytes":data}));
                }
            }
            replies.push(json!({"request":0xe0,"count":terminal,"bytes":[]}));
            output
                .push(json!({"replies":replies,"operations":[{"op":"serial_read","port":65535}]}));
        }
    }
    output
}
