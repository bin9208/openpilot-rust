use base64::{engine::general_purpose::STANDARD, Engine};
use num_traits::ToPrimitive;
use openpilot_carrot_server::{
    youtube_live::{captions::Injector, flv::Muxer, h264, profiles},
    Error, Value,
};
use std::{
    io::{self, BufRead},
    path::Path,
};

fn bytes(value: &Value) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(STANDARD.decode(value.string()?)?)
}
fn captured(result: Result<Vec<u8>, Error>) -> Value {
    match result {
        Ok(bytes) => Value::object([
            ("bytes", Value::text(&STANDARD.encode(bytes))),
            ("error", Value::Null),
        ]),
        Err(error) => Value::object([
            ("bytes", Value::Null),
            ("error", Value::text(&error.to_string())),
        ]),
    }
}
fn array(value: &Value) -> Result<&[Value], Box<dyn std::error::Error>> {
    match value {
        Value::Array(items) => Ok(items),
        _ => Err("expected fixture array".into()),
    }
}
fn pure(input: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    let configurations = array(input.get("configs"))?
        .iter()
        .map(|value| Ok(captured(h264::configuration(&bytes(value)?))))
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let access = array(input.get("access"))?
        .iter()
        .map(|value| {
            let result = match h264::normalize(&bytes(value)?) {
                Ok(unit) => Value::object([
                    ("bytes", Value::text(&STANDARD.encode(unit.avcc))),
                    (
                        "nal_types",
                        Value::Array(unit.nal_types.into_iter().map(Value::integer).collect()),
                    ),
                    ("error", Value::Null),
                ]),
                Err(error) => Value::object([
                    ("bytes", Value::Null),
                    ("nal_types", Value::Null),
                    ("error", Value::text(&error.to_string())),
                ]),
            };
            Ok(result)
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let starts = array(input.get("starts"))?
        .iter()
        .map(|value| {
            Ok(
                match h264::validate_start(
                    &bytes(value.get("header"))?,
                    &bytes(value.get("payload"))?,
                ) {
                    Ok(()) => Value::Null,
                    Err(error) => Value::text(&error.to_string()),
                },
            )
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let selected = array(input.get("qualities"))?
        .iter()
        .map(|value| {
            let quality = value.int()?.to_i32().unwrap_or(-1);
            let profile = profiles::selected(quality);
            Ok(Value::object([
                ("quality", Value::integer(profile.quality)),
                ("label", Value::text(profile.label)),
                ("source", Value::text(profiles::SOURCE)),
                ("process", Value::text(profile.process)),
                ("encoder_flag", Value::text(profile.encoder_flag)),
                ("target", profile.target()),
            ]))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let mut injector = Injector::default();
    let mut captions = Vec::new();
    for row in array(input.get("captions"))? {
        if row.get("reset").truth() {
            injector.reset();
        }
        let payload = bytes(row.get("payload"))?;
        let result = injector.inject(
            &payload,
            row.get("enabled").truth(),
            &row.get("text").string()?,
        )?;
        captions.push(Value::object([
            ("bytes", Value::text(&STANDARD.encode(result))),
            ("packets", Value::integer(injector.packets)),
        ]));
    }
    Ok(Value::object([
        ("configs", Value::Array(configurations)),
        ("access", Value::Array(access)),
        ("starts", Value::Array(starts)),
        ("profiles", Value::Array(selected)),
        ("captions", Value::Array(captions)),
    ]))
}
fn mux(input: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    let header = std::fs::read(input.get("header").string()?)?;
    let fps = input.get("fps").int()?.to_u32().unwrap_or(1);
    let mut muxer = Muxer::new(Vec::new(), &header, fps)?;
    for row in array(input.get("frames"))? {
        let data = std::fs::read(row.get("path").string()?)?;
        let timestamp = if matches!(row.get("timestamp_ms"), Value::Null) {
            None
        } else {
            row.get("timestamp_ms").int()?.to_i64()
        };
        muxer.mux(&data, row.get("keyframe").truth(), timestamp)?;
    }
    muxer.close()?;
    let bytes = muxer.output();
    std::fs::write(Path::new(&input.get("output").string()?), bytes)?;
    Ok(Value::object([("bytes", Value::integer(bytes.len()))]))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let output = if input.get("mode").string()? == "pure" {
        pure(&input)?
    } else {
        mux(&input)?
    };
    println!("{}", output.encode()?);
    Ok(())
}
