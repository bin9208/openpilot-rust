use super::{encoding, Error, VisionResult};
use serde_json::value::RawValue;
use std::collections::BTreeMap;

type Object = BTreeMap<String, Box<RawValue>>;

fn object(text: &str, field: &'static str) -> Result<Object, Error> {
    if !text.trim_start().starts_with('{') {
        return Err(Error::Field(field));
    }
    Ok(serde_json::from_str(text)?)
}
fn field<'a>(object: &'a Object, name: &'static str) -> Result<&'a str, Error> {
    object
        .get(name)
        .map(|value| value.get())
        .ok_or(Error::Field(name))
}

fn lane(value: &str, field: &'static str) -> Result<i16, Error> {
    match value.parse() {
        Ok(-1) => Ok(-1),
        Ok(0) => Ok(0),
        Ok(1) => Ok(1),
        _ => Err(Error::Field(field)),
    }
}
fn boolean(value: &str, field: &'static str) -> Result<bool, Error> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(Error::Field(field)),
    }
}
fn timestamp(text: &str, field: &'static str) -> Result<Option<u64>, Error> {
    if text == "-0" {
        return Ok(Some(0));
    }
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::Field(field));
    }
    Ok(text.parse().ok())
}

pub fn parse(bytes: &[u8]) -> Result<VisionResult, Error> {
    let text = encoding::decode(bytes)?;
    encoding::integer_limit(&text)?;
    let data = object(&encoding::python_literals(&text), "header")?;
    let version = field(&data, "version")?;
    let version_one = version == "true" || version.parse::<f64>() == Ok(1.);
    let tag: String = serde_json::from_str(field(&data, "type")?)?;
    if tag != "xiaogeVision" || !version_one {
        return Err(Error::Field("header"));
    }
    let left = object(field(&data, "lane")?, "lane/blindspot")?;
    let blindspot = object(field(&data, "blindspot")?, "lane/blindspot")?;
    Ok(VisionResult {
        left_lane: lane(field(&left, "leftLine")?, "lane.leftLine")?,
        right_lane: lane(field(&left, "rightLine")?, "lane.rightLine")?,
        lane_valid: boolean(field(&left, "valid")?, "lane.valid")?,
        lane_received: timestamp(
            field(&left, "receivedMonoTimeNanos")?,
            "lane.receivedMonoTimeNanos",
        )?,
        left_blindspot: boolean(field(&blindspot, "left")?, "blindspot.left")?,
        right_blindspot: boolean(field(&blindspot, "right")?, "blindspot.right")?,
        blindspot_valid: boolean(field(&blindspot, "valid")?, "blindspot.valid")?,
        blindspot_received: timestamp(
            field(&blindspot, "receivedMonoTimeNanos")?,
            "blindspot.receivedMonoTimeNanos",
        )?,
    })
}
