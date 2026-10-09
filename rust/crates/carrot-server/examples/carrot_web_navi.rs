use base64::{engine::general_purpose::STANDARD, Engine};
use num_traits::ToPrimitive;
use openpilot_carrot_server::{
    web_navi::fmp4::{Muxer, Sample},
    Value,
};
use std::{
    error::Error,
    io::{self, BufRead},
    path::Path,
};
#[path = "carrot_web_navi/server.rs"]
mod server;

fn integer(value: &Value) -> Result<u64, Box<dyn Error>> {
    value
        .int()?
        .to_u64()
        .ok_or_else(|| "expected nonnegative u64".into())
}
fn run(input: Value) -> Result<Value, Box<dyn Error>> {
    let config = std::fs::read(input.get("config").string()?)?;
    let width = u32::try_from(integer(input.get("width"))?)?;
    let height = u32::try_from(integer(input.get("height"))?)?;
    let mut muxer = Muxer::default();
    muxer.configure(&config, (width, height), &input.get("session").string()?);
    let Value::Array(frames) = input.get("frames") else {
        return Err("frames must be an array".into());
    };
    let mut rows = Vec::new();
    for frame in frames {
        let path = frame.get("path").string()?;
        let bytes = std::fs::read(Path::new(&path))?;
        let output = muxer.push(Sample {
            payload: &bytes,
            sequence: integer(frame.get("sequence"))?,
            timestamp_ms: integer(frame.get("timestamp_ms"))?,
            keyframe: frame.get("keyframe").truth(),
        })?;
        let initialization = output.initialization.map_or(Value::Null, |init| {
            Value::object([
                ("payload", Value::text(&STANDARD.encode(init.payload))),
                ("mime", Value::text(&init.mime)),
                ("width", Value::integer(init.width)),
                ("height", Value::integer(init.height)),
            ])
        });
        let segments = output
            .segments
            .into_iter()
            .map(|segment| {
                Value::object([
                    ("payload", Value::text(&STANDARD.encode(segment.payload))),
                    ("sequence", Value::integer(segment.sequence)),
                    ("timestamp_ms", Value::integer(segment.timestamp_ms)),
                    ("duration_ms", Value::integer(segment.duration_ms)),
                    ("keyframe", Value::Bool(segment.keyframe)),
                ])
            })
            .collect();
        rows.push(Value::object([
            ("initialization", initialization),
            ("segments", Value::Array(segments)),
        ]));
    }
    Ok(Value::Array(rows))
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let line = io::stdin().lock().lines().next().ok_or("missing input")??;
    let input = Value::parse(&line)?;
    if input.has("config") {
        println!("{}", run(input)?.encode()?);
    } else {
        server::run(input).await?;
    }
    Ok(())
}
