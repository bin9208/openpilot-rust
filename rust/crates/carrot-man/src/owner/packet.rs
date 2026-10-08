use super::{route_payload, traffic_payload};
use crate::{
    navigation::{parse_legacy, Auxiliary, LegacyFields},
    Error,
};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Phone {
    pub latitude: Result<Option<f64>, &'static str>,
    pub longitude: Result<Option<f64>, &'static str>,
    pub heading: Result<Option<f64>, &'static str>,
    pub accuracy: Result<f64, &'static str>,
    pub speed: Result<f64, &'static str>,
}
#[derive(Clone, Debug)]
pub struct Status {
    pub index: Option<i64>,
    pub command: Option<(String, String, bool, bool, bool)>,
    pub control: Option<LegacyFields>,
    pub phone: Option<Phone>,
    pub epoch: Option<(Result<i64, &'static str>, String)>,
}
#[derive(Clone, Debug)]
pub struct LegacyFrame {
    pub timestamp: i64,
    pub event_type: &'static str,
    pub status: Option<Result<Status, &'static str>>,
    pub auxiliary: Vec<Auxiliary>,
    pub image: Option<Image>,
    pub record: super::events::EventRecord,
    pub debug: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Image {
    pub parameter: String,
    pub summary: String,
}

pub(crate) fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}
pub(super) fn float(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(if *b { 1. } else { 0. }),
        Value::String(s) => openpilot_runtime_core::python_float::parse(s),
        _ => None,
    }
}
pub(super) fn integer(value: &Value) -> Option<i64> {
    if let Value::String(s) = value {
        return s.trim().replace('_', "").parse().ok();
    }
    float(value).and_then(|v| num_traits::ToPrimitive::to_i64(&v.trunc()))
}
pub(crate) fn py_text(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}

pub fn status(value: &Value, now: f64) -> Result<Status, Error> {
    let index = value
        .get("carrotIndex")
        .map(|v| {
            if truth(v) {
                integer(v).ok_or(Error::Contract("invalid carrot index"))
            } else {
                Ok(0)
            }
        })
        .transpose()?;
    let command = value.get("carrotCmd").map(|command| {
        (
            py_text(command),
            value.get("carrotArg").map_or("None".into(), py_text),
            command.is_string(),
            value.get("carrotArg").is_some_and(Value::is_string),
            !matches!(command, Value::Object(_) | Value::Array(_)),
        )
    });
    let phone = value.get("latitude").map(|_| {
        let optional = |name: &str| -> Result<Option<f64>, &'static str> {
            value
                .get(name)
                .filter(|v| !v.is_null())
                .map(|v| float(v).ok_or("invalid phone coordinate"))
                .transpose()
        };
        Phone {
            latitude: optional("latitude"),
            longitude: optional("longitude"),
            heading: optional("heading"),
            accuracy: optional("accuracy").map(|v| v.unwrap_or(0.)),
            speed: value
                .get("gps_speed")
                .map(float)
                .unwrap_or(Some(0.))
                .ok_or("invalid phone speed"),
        }
    });
    let epoch = value
        .get("epochTime")
        .filter(|v| !v.is_null())
        .map(|epoch| {
            (
                integer(epoch).ok_or("invalid epoch time"),
                value.get("timezone").map_or("Asia/Seoul".into(), py_text),
            )
        });
    Ok(Status {
        index,
        command,
        control: parse_legacy(value, now),
        phone,
        epoch,
    })
}

pub fn frame(value: &Value, now: f64, wall: String, session: &str) -> Result<LegacyFrame, Error> {
    let event_type = [
        "complexCrossroad",
        "rgdata",
        "vrtx",
        "ssinf",
        "sinf",
        "route",
    ]
    .into_iter()
    .find(|key| value.get(*key).is_some_and(|v| !v.is_null()))
    .unwrap_or("unknown");
    let timestamp = value
        .get("timestamp_ms")
        .filter(|v| truth(v))
        .or_else(|| value.get("timestamp").filter(|v| truth(v)))
        .and_then(integer)
        .unwrap_or(0);
    let normalized = value.get("rgdata").filter(|v| v.is_object()).map(|raw| {
        let mut merged = raw.clone();
        if let Some(map) = merged.as_object_mut() {
            for group in ["guidance", "sdi", "lane"] {
                if let Some(values) = raw.get(group).and_then(Value::as_object) {
                    for (key, value) in values {
                        map.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }
            }
            map.insert("_navigation_source".into(), "tmap_legacy".into());
            map.insert("_navigation_session_id".into(), session.into());
            map.insert("_navigation_received_mono_s".into(), now.into());
        }
        merged
    });
    let status = normalized
        .as_ref()
        .map(|v| status(v, now).map_err(|_| "legacy status conversion failed"));
    let mut auxiliary = Vec::new();
    for name in ["vrtx", "route"] {
        if let Some(payload) = value.get(name) {
            if let Some(points) = route_payload::route_points(payload, 0) {
                let points = openpilot_navd::geometry::limit_route_points(&points, 4096)?;
                auxiliary.push(Auxiliary {
                    route: Some(points.into_iter().map(|p| (p.1, p.0)).collect()),
                    ..Auxiliary::default()
                });
            }
        }
    }
    for name in ["sinf", "ssinf"] {
        if let Some(payload) = value.get(name) {
            auxiliary.push(Auxiliary {
                traffic: Some(
                    traffic_payload::traffic(payload, name == "ssinf").unwrap_or_default(),
                ),
                ..Auxiliary::default()
            });
        }
    }
    let image = value
        .get("complexCrossroad")
        .filter(|v| v.is_object())
        .map(|v| super::events::image(v, now))
        .transpose()?;
    let record = super::events::record(value, event_type, timestamp, wall)?;
    let debug =
        if status.is_some() || !auxiliary.is_empty() || value.get("complexCrossroad").is_some() {
            Some(super::events::debug(
                value,
                normalized.as_ref(),
                event_type,
                timestamp,
                now,
            )?)
        } else {
            None
        };
    Ok(LegacyFrame {
        timestamp,
        event_type,
        status,
        auxiliary,
        image,
        record,
        debug,
    })
}
