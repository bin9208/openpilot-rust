use openpilot_carrot_server::{dashcam::build_report, Error, Value};
#[path = "../src/dashcam/report/codec.rs"]
mod codec;
#[path = "../src/dashcam/report/format.rs"]
mod format;
use std::{
    io::{self, BufRead},
    path::Path,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let input = Value::parse(&line?)?;
        let output = match input.get("operation").string()?.as_str() {
            "codec" => match codec::decompress(Path::new(&input.get("file").string()?)) {
                Ok(data) => {
                    use sha2::Digest;
                    Value::object([
                        ("ok", Value::Bool(true)),
                        ("bytes", Value::integer(data.len())),
                        (
                            "sha256",
                            Value::text(&format!("{:x}", sha2::Sha256::digest(&data))),
                        ),
                    ])
                }
                Err(error) => Value::object([
                    ("ok", Value::Bool(false)),
                    ("error", Value::text(&error.to_string())),
                ]),
            },
            "report" => build_report(
                Path::new(&input.get("root").string()?),
                input.get("route"),
                input.get("prefer_rlog").truth(),
            )?,
            "format" => {
                let value = input.get("value").float()?;
                Value::object([
                    ("hms", Value::text(&format::hms(value)?)),
                    ("ms", Value::text(&format::ms(value)?)),
                    ("clock", Value::text(&format::clock(value)?)),
                    ("round2", format::number(value, 2)),
                    ("round1", format::number(value, 1)),
                ])
            }
            other => return Err(format!("unknown report fixture operation: {other}").into()),
        };
        println!("{}", output.encode()?);
    }
    Ok(())
}
