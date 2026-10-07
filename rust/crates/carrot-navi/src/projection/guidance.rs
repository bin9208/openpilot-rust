use super::numeric::*;
use crate::{json::Value, Error};

pub(super) fn guidance(snapshot: &Value, name: &str) -> Result<Value, Error> {
    let record = record(snapshot, name);
    let value = present_value(record);
    let point = dict(value.get("point"));
    Ok(Value::object([
        ("meta", meta(record)?),
        ("distanceM", integer0(value.get("distance_m"), 2_000_000)?),
        ("timeSec", integer0(value.get("time_sec"), 604_800)?),
        (
            "turnType",
            integer(value.get("turn_type"), -1, Some(-1), Some(100_000))?,
        ),
        ("roadName", text(value.get("road_name"), 96)?),
        ("mainText", text(value.get("main_text"), 160)?),
        ("nearDirection", text(value.get("near_direction"), 96)?),
        ("midDirection", text(value.get("mid_direction"), 96)?),
        ("farDirection", text(value.get("far_direction"), 96)?),
        (
            "pointValid",
            Value::Bool(
                !matches!(point.get("lat"), Value::Null)
                    && !matches!(point.get("lon"), Value::Null),
            ),
        ),
        (
            "latitude",
            Value::Float(finite(point.get("lat"), 0., Some(-90.), Some(90.))?),
        ),
        (
            "longitude",
            Value::Float(finite(point.get("lon"), 0., Some(-180.), Some(180.))?),
        ),
    ]))
}
fn int16_list(value: &Value) -> Result<Value, Error> {
    Ok(Value::Array(
        list(value)
            .iter()
            .take(16)
            .map(|value| integer(value, 0, Some(-32768), Some(32767)))
            .collect::<Result<_, _>>()?,
    ))
}
pub(super) fn lane(record: &Value, value: &Value) -> Result<Value, Error> {
    let lane = dict(value);
    Ok(Value::object([
        ("meta", meta(record)?),
        ("count", integer0(lane.get("count"), 16)?),
        ("distanceM", integer0(lane.get("distance_m"), 2_000_000)?),
        (
            "visible",
            Value::Bool(if lane.has("visible") {
                lane.get("visible").truth()
            } else {
                true
            }),
        ),
        ("lanePlay", Value::Bool(lane.get("lane_play").truth())),
        (
            "currentLane",
            integer(lane.get("current_lane"), -1, Some(-1), Some(16))?,
        ),
        (
            "turnCode",
            integer(lane.get("turn_code"), -1, Some(-1), Some(100_000))?,
        ),
        ("turnInfo", int16_list(lane.get("turn_info"))?),
        ("etcInfo", int16_list(lane.get("etc_info"))?),
        ("available", int16_list(lane.get("available"))?),
        (
            "guideLineColor",
            integer(lane.get("guide_line_color"), 0, Some(-32768), Some(32767))?,
        ),
        (
            "roadCategory",
            integer(lane.get("road_category"), 0, Some(-32768), Some(32767))?,
        ),
        (
            "voiceCode",
            integer(lane.get("voice_code"), 0, Some(-32768), Some(32767))?,
        ),
    ]))
}
pub(super) fn polyline(snapshot: &Value) -> Result<Value, Error> {
    let record = record(snapshot, "route");
    let value = present_value(record);
    let mut polyline = Vec::new();
    for point in list(value.get("polyline")).iter().take(256) {
        let point = dict(point);
        if matches!(point.get("lat"), Value::Null) || matches!(point.get("lon"), Value::Null) {
            continue;
        }
        polyline.push(Value::object([
            (
                "latitude",
                Value::Float(finite(point.get("lat"), 0., Some(-90.), Some(90.))?),
            ),
            (
                "longitude",
                Value::Float(finite(point.get("lon"), 0., Some(-180.), Some(180.))?),
            ),
        ]));
    }
    Ok(Value::Array(polyline))
}
pub(super) fn route(snapshot: &Value, polyline: Value) -> Result<Value, Error> {
    let record = record(snapshot, "route");
    let value = present_value(record);
    Ok(Value::object([
        ("meta", meta(record)?),
        (
            "remainingDistanceM",
            integer0(value.get("remain_distance_m"), 2_000_000)?,
        ),
        (
            "remainingTimeSec",
            integer0(value.get("remain_time_sec"), 604_800)?,
        ),
        (
            "movedDistanceM",
            integer0(value.get("moved_distance_m"), 2_000_000)?,
        ),
        (
            "movedTimeSec",
            integer0(value.get("moved_time_sec"), 604_800)?,
        ),
        (
            "totalDistanceM",
            integer0(value.get("total_distance_m"), 2_000_000)?,
        ),
        ("polyline", polyline),
    ]))
}
