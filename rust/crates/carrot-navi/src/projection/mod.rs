mod guidance;
mod numeric;
mod speed;
mod traffic;

use crate::{json::Value, record::Record, Error};
use numeric::*;
use std::sync::Arc;

fn vehicle(snapshot: &Value) -> Result<Value, Error> {
    let record = record(snapshot, "vehicle");
    let value = present_value(record);
    Ok(Value::object([
        ("meta", meta(record)?),
        (
            "latitude",
            Value::Float(finite(value.get("lat"), 0., Some(-90.), Some(90.))?),
        ),
        (
            "longitude",
            Value::Float(finite(value.get("lon"), 0., Some(-180.), Some(180.))?),
        ),
        ("headingDeg", number0(value.get("heading_deg"), 360.)?),
        ("speedKph", number0(value.get("speed_kph"), 300.)?),
        ("roadName", text(value.get("road_name"), 96)?),
        ("virtualGps", Value::Bool(value.get("virtual_gps").truth())),
    ]))
}
fn crossroad(snapshot: &Value) -> Result<Value, Error> {
    let record = record(snapshot, "crossroad");
    let value = present_value(record);
    Ok(Value::object([
        ("meta", meta(record)?),
        ("visible", Value::Bool(value.get("visible").truth())),
        ("distanceM", integer0(value.get("distance_m"), 100_000)?),
        (
            "imageCode",
            integer0(value.get("image_code"), 2_147_483_647)?,
        ),
        ("imageUrl", text(value.get("image_url"), 512)?),
    ]))
}
fn status(snapshot: &Value) -> Result<Value, Error> {
    let record = record(snapshot, "navigation_status");
    let value = present_value(record);
    Ok(Value::object([
        ("meta", meta(record)?),
        ("mode", text(value.get("mode"), 32)?),
        (
            "guidanceActive",
            Value::Bool(value.get("guidance_active").truth()),
        ),
        ("offRoute", Value::Bool(value.get("off_route").truth())),
        (
            "routePresent",
            Value::Bool(value.get("route_present").truth()),
        ),
    ]))
}

pub fn payload(snapshot: &Value, publish_mono_ns: impl FnOnce() -> u128) -> Result<Value, Error> {
    let lane_record = record(snapshot, "lane_current");
    let lane_value = if lane_record.get("present").truth() {
        lane_record.get("value")
    } else {
        &Value::Null
    };
    let ahead_record = record(snapshot, "lane_ahead");
    let ahead = if ahead_record.get("present").truth() {
        list(ahead_record.get("value"))
    } else {
        &[]
    };
    let limit = valid_road_limit(present_value(record(snapshot, "speed")).get("road_limit_kph"))?;
    let lights = traffic::lights(snapshot)?;
    let polyline = guidance::polyline(snapshot)?;
    Ok(Value::object([
        ("schemaVersion", Value::integer(1)),
        (
            "generation",
            integer(snapshot.get("generation"), 0, Some(0), None)?,
        ),
        ("sessionId", text(snapshot.get("session_id"), 32)?),
        ("publishMonoTimeNanos", Value::integer(publish_mono_ns())),
        ("connected", Value::Bool(snapshot.get("connected").truth())),
        ("vehicle", vehicle(snapshot)?),
        (
            "guidanceCurrent",
            guidance::guidance(snapshot, "guidance_current")?,
        ),
        (
            "guidanceNext",
            guidance::guidance(snapshot, "guidance_next")?,
        ),
        ("laneCurrent", guidance::lane(lane_record, lane_value)?),
        (
            "laneAhead",
            Value::Array(
                ahead
                    .iter()
                    .take(8)
                    .map(|value| guidance::lane(ahead_record, value))
                    .collect::<Result<_, _>>()?,
            ),
        ),
        ("speed", speed::speed(snapshot, limit)?),
        ("trafficSignal", traffic::traffic(snapshot, lights)?),
        ("crossroad", crossroad(snapshot)?),
        ("route", guidance::route(snapshot, polyline)?),
        ("navigationStatus", status(snapshot)?),
    ]))
}

pub struct MediaPayload {
    pub metadata: Value,
    pub payload: Arc<[u8]>,
}

pub fn media(record: &Record, session: &Value) -> Result<MediaPayload, Error> {
    Ok(MediaPayload {
        metadata: Value::object([
            ("schemaVersion", Value::integer(1)),
            ("sessionId", text(session, 32)?),
            ("kind", text(&Value::text(&record.kind), 16)?),
            ("name", text(&Value::text(&record.name), 64)?),
            (
                "sequence",
                integer(&Value::Integer(record.sequence.clone()), 0, Some(0), None)?,
            ),
            (
                "sourceTimestampMillis",
                integer(
                    &Value::Integer(record.source_timestamp_ms.clone()),
                    0,
                    Some(0),
                    None,
                )?,
            ),
            (
                "receivedMonoTimeNanos",
                integer(&Value::integer(record.received_mono_ns), 0, Some(0), None)?,
            ),
            ("present", Value::Bool(record.present)),
            (
                "messageType",
                integer0(
                    &record
                        .message_type
                        .clone()
                        .map(Value::Integer)
                        .unwrap_or(Value::Null),
                    255,
                )?,
            ),
            (
                "formatOrReason",
                integer0(
                    &record
                        .format_or_reason
                        .clone()
                        .map(Value::Integer)
                        .unwrap_or(Value::Null),
                    255,
                )?,
            ),
            (
                "flags",
                integer0(&Value::Integer(record.flags.clone()), 65535)?,
            ),
            (
                "width",
                integer0(&Value::Integer(record.width.clone()), 65535)?,
            ),
            (
                "height",
                integer0(&Value::Integer(record.height.clone()), 65535)?,
            ),
            (
                "reason",
                text(record.reason.as_ref().unwrap_or(&Value::Null), 64)?,
            ),
        ]),
        payload: record
            .payload
            .as_ref()
            .map(Arc::clone)
            .unwrap_or_else(|| Arc::from([])),
    })
}
