use super::numeric::*;
use crate::{json::Value, Error};

pub(super) fn speed(snapshot: &Value, limit: Option<Value>) -> Result<Value, Error> {
    let record = record(snapshot, "speed");
    let value = present_value(record);
    let sdi = dict(value.get("sdi"));
    let secondary = dict(value.get("sdi_secondary"));
    let section = dict(value.get("section"));
    Ok(Value::object([
        ("meta", meta(record)?),
        ("currentKph", number0(value.get("current_kph"), 300.)?),
        ("roadLimitValid", Value::Bool(limit.is_some())),
        (
            "roadLimitKph",
            integer0(limit.as_ref().unwrap_or(&Value::Null), 200)?,
        ),
        ("sdiPresent", Value::Bool(sdi.truth())),
        (
            "sdiType",
            integer(sdi.get("type"), -1, Some(-1), Some(100_000))?,
        ),
        ("sdiDistanceM", integer0(sdi.get("distance_m"), 2_000_000)?),
        (
            "sdiSpeedLimitKph",
            integer0(sdi.get("speed_limit_kph"), 300)?,
        ),
        (
            "sdiSectionType",
            integer(sdi.get("section_type"), -1, Some(-1), Some(100_000))?,
        ),
        (
            "sdiBlockType",
            integer(sdi.get("block_type"), -1, Some(-1), Some(100_000))?,
        ),
        (
            "sdiBlockSpeedKph",
            integer0(sdi.get("block_speed_kph"), 300)?,
        ),
        (
            "sdiBlockDistanceM",
            integer0(sdi.get("block_distance_m"), 2_000_000)?,
        ),
        ("secondarySdiPresent", Value::Bool(secondary.truth())),
        (
            "secondarySdiType",
            integer(secondary.get("type"), -1, Some(-1), Some(100_000))?,
        ),
        (
            "secondarySdiDistanceM",
            integer0(secondary.get("distance_m"), 2_000_000)?,
        ),
        (
            "secondarySdiSpeedLimitKph",
            integer0(secondary.get("speed_limit_kph"), 300)?,
        ),
        (
            "secondarySdiSectionType",
            integer(secondary.get("section_type"), -1, Some(-1), Some(100_000))?,
        ),
        (
            "secondarySdiBlockType",
            integer(secondary.get("block_type"), -1, Some(-1), Some(100_000))?,
        ),
        (
            "secondarySdiBlockSpeedKph",
            integer0(secondary.get("block_speed_kph"), 300)?,
        ),
        (
            "secondarySdiBlockDistanceM",
            integer0(secondary.get("block_distance_m"), 2_000_000)?,
        ),
        ("sectionPresent", Value::Bool(section.truth())),
        ("sectionActive", Value::Bool(section.get("active").truth())),
        (
            "sectionSpeedLimitKph",
            integer0(section.get("speed_limit_kph"), 300)?,
        ),
        (
            "sectionAverageKph",
            number0(section.get("average_kph"), 300.)?,
        ),
        (
            "sectionOverallAverageKph",
            number0(section.get("overall_average_kph"), 300.)?,
        ),
        (
            "sectionRemainingDistanceM",
            number0(section.get("remaining_distance_m"), 2_000_000.)?,
        ),
        (
            "sectionRemainingTimeSec",
            integer0(section.get("remaining_time_sec"), 604_800)?,
        ),
        ("sectionProgress", number0(section.get("progress"), 1.)?),
        (
            "sectionSuspended",
            Value::Bool(section.get("suspended").truth()),
        ),
        (
            "sectionOffRoute",
            Value::Bool(section.get("off_route").truth()),
        ),
    ]))
}
