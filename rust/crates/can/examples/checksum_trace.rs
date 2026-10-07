use openpilot_can::{checksum::Kind, signal::Signal};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

#[derive(Deserialize)]
struct Case {
    dbc: String,
    address: u32,
    start_bit: usize,
    data: Vec<u8>,
}
#[derive(Serialize)]
struct Output {
    checksum: Option<u16>,
    data: Vec<u8>,
    error: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let result: Vec<_> = cases
        .into_iter()
        .map(|case| {
            let signal = Signal {
                name: "CHECKSUM".into(),
                start_bit: case.start_bit,
                msb: case.start_bit,
                lsb: case.start_bit,
                size: 8,
                signed: false,
                factor: 1.,
                offset: 0.,
                little_endian: true,
                kind: Kind::for_signal(&case.dbc, "CHECKSUM"),
            };
            let mut data = case.data;
            match signal.kind.compute(case.address, &signal, &mut data) {
                Ok(checksum) => Output {
                    checksum: Some(checksum),
                    data,
                    error: None,
                },
                Err(error) => Output {
                    checksum: None,
                    data,
                    error: Some(error.to_string()),
                },
            }
        })
        .collect();
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    std::fs::write(path, serde_json::to_vec(&result)?)?;
    Ok(())
}
