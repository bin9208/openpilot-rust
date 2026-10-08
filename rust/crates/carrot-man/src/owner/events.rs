use super::{
    packet::{float, py_text, truth, Image},
    route_payload::{route_points, route_summary},
};
use crate::Error;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRecord {
    pub received_at: String,
    pub event_time_ms: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub summary: Value,
}
#[derive(Default, Debug)]
pub struct EventState {
    pub last: Option<EventRecord>,
    pub by_type: BTreeMap<String, EventRecord>,
    pub crossroad_summary: Option<String>,
}
impl EventState {
    pub fn record(&mut self, event: EventRecord) {
        self.by_type.insert(event.event_type.clone(), event.clone());
        self.last = Some(event);
    }
    pub fn health(&self) -> Value {
        json!({"ok":true,"service":"carrot_navi_http","lastEvent":self.last.as_ref().map(|e| json!({"receivedAt":e.received_at,"eventTimeMs":e.event_time_ms,"summary":e.summary})), "receivedTypes":self.by_type.keys().collect::<Vec<_>>()})
    }
}

fn safe_int(value: Option<&Value>, range: (i64, i64)) -> Option<i64> {
    let value = float(value?)?.trunc();
    let value = num_traits::ToPrimitive::to_i64(&value)?;
    (range.0..=range.1).contains(&value).then_some(value)
}
fn safe_float(value: Option<&Value>, range: (f64, f64)) -> Option<f64> {
    let value = float(value?)?;
    (value.is_finite() && (range.0..=range.1).contains(&value)).then_some(value)
}
fn text(value: &Value, field: &str) -> String {
    value.get(field).map_or(String::new(), py_text)
}

pub fn record(
    value: &Value,
    kind: &str,
    timestamp: i64,
    wall: String,
) -> Result<EventRecord, Error> {
    let mut summary = json!({"type":kind});
    match (kind, value.get(kind)) {
        ("rgdata", Some(rg)) if rg.is_object() => {
            for (target, source) in [
                ("lat", "vpPosPointLat"),
                ("lon", "vpPosPointLon"),
                ("speed", "nPosSpeed"),
                ("roadLimitSpeed", "nRoadLimitSpeed"),
                ("tbtDist", "nTBTDist"),
                ("tbtTurnType", "nTBTTurnType"),
                ("sdiType", "nSdiType"),
                ("sdiDist", "nSdiDist"),
            ] {
                summary[target] = rg.get(source).cloned().unwrap_or(Value::Null);
            }
        }
        ("vrtx" | "route", Some(route)) => {
            if let Some(map) = route_summary(route).as_object() {
                for (k, v) in map {
                    summary[k] = v.clone();
                }
            }
        }
        ("sinf", Some(signal)) if signal.is_object() => {
            for field in ["distance", "redLightOn", "greenLightOn", "leftLightOn"] {
                summary[field] = signal.get(field).cloned().unwrap_or(Value::Null);
            }
        }
        ("ssinf", Some(signal)) if signal.is_object() => {
            for (target, field) in [
                ("distance", "distance"),
                ("straight", "straight"),
                ("left", "left"),
                ("straightRemain", "straight_remain_time"),
                ("leftRemain", "left_remain_time"),
            ] {
                summary[target] = signal.get(field).cloned().unwrap_or(Value::Null);
            }
        }
        ("complexCrossroad", Some(crossroad)) if crossroad.is_object() => {
            let base64 = crossroad.get("imageBase64").and_then(Value::as_str);
            summary = json!({"type":kind,"show":crossroad.get("show").is_some_and(truth),"imageUrl":text(crossroad,"imageUrl").chars().take(200).collect::<String>(),"imageMime":text(crossroad,"imageMime").chars().take(64).collect::<String>(),
                "imageWidth":safe_int(crossroad.get("imageWidth"),(0,i64::MAX)).unwrap_or(0),"imageHeight":safe_int(crossroad.get("imageHeight"),(0,i64::MAX)).unwrap_or(0),"totalMeters":safe_int(crossroad.get("totalMeters"),(0,i64::MAX)).unwrap_or(0),
                "remainRatio":safe_float(crossroad.get("remainRatio"),(0.,1.)).unwrap_or(0.),"hasImageBase64":base64.is_some_and(|s| !s.is_empty()),"imageBase64Size":base64.map_or(0,|s|s.chars().count())});
        }
        _ => {
            summary["keys"] = json!(value
                .as_object()
                .map(|o| o.keys().take(10).collect::<Vec<_>>())
                .unwrap_or_default());
        }
    }
    Ok(EventRecord {
        received_at: wall,
        event_time_ms: timestamp,
        event_type: kind.into(),
        summary,
    })
}

pub fn image(value: &Value, now: f64) -> Result<Image, Error> {
    let base64 = value
        .get("imageBase64")
        .and_then(Value::as_str)
        .unwrap_or("");
    let hash = if base64.is_empty() {
        String::new()
    } else {
        let ascii: Vec<_> = base64
            .chars()
            .filter(char::is_ascii)
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(0))
            .collect();
        format!("{:x}", Sha256::digest(ascii))[..16].to_owned()
    };
    let large = base64.chars().count() > 6 * 1024 * 1024;
    let width = safe_int(value.get("imageWidth"), (0, i64::MAX)).unwrap_or(0);
    let height = safe_int(value.get("imageHeight"), (0, i64::MAX)).unwrap_or(0);
    let show = value.get("show").is_some_and(truth);
    let url = text(value, "imageUrl");
    let mime = text(value, "imageMime");
    let encoding = text(value, "imageEncoding");
    let summary = json!({"show":show,"imageUrl":url,"imageMime":mime,"imageEncoding":encoding,"imageWidth":width,"imageHeight":height,
        "totalMeters":safe_int(value.get("totalMeters"),(0,i64::MAX)).unwrap_or(0),"remainRatio":safe_float(value.get("remainRatio"),(0.,1.)).unwrap_or(0.),"imageHash":hash,"ts":now});
    let parameter = json!({"receivedMono":now,"show":show,"imageBase64":if large { "" } else { base64 },"imageMime":mime,"imageEncoding":encoding,
        "imageWidth":width,"imageHeight":height,"imageHash":hash,"imageUrl":url,"imageTooLarge":large});
    Ok(Image {
        parameter: serde_json::to_string(&parameter)?,
        summary: serde_json::to_string(&summary)?,
    })
}

fn line(label: &str, value: String) -> String {
    format!("{label}: {value}").chars().take(120).collect()
}
fn display(value: &Value, field: &str) -> String {
    value.get(field).map_or("--".into(), py_text)
}
fn traffic_debug(value: &Value, detailed: bool) -> Value {
    let names = if detailed {
        [
            "straight_remain_time",
            "left_remain_time",
            "right_remain_time",
            "uturn_remain_time",
        ]
    } else {
        [
            "greenLightRemainTime",
            "leftLightRemainTime",
            "rightLightRemainTime",
            "uturnLightRemainTime",
        ]
    };
    let signals = if detailed {
        ["straight", "left", "right", "uturn"]
    } else {
        [
            "greenLightOn",
            "leftLightOn",
            "rightLightOn",
            "uturnLightOn",
        ]
    };
    let active = |field: &str| {
        value.get(field).is_some_and(|v| {
            if detailed {
                py_text(v).to_uppercase() == "GREEN_LIGHT_ON"
            } else {
                truth(v)
            }
        })
    };
    let red = if detailed {
        signals
            .iter()
            .filter(|field| {
                value
                    .get(**field)
                    .is_some_and(|v| py_text(v).to_uppercase() == "RED_LIGHT_ON")
            })
            .filter_map(|field| safe_int(value.get(format!("{field}_remain_time")), (1, 999)))
            .max()
    } else {
        safe_int(value.get("redLightRemainTime"), (1, 999))
    };
    json!({"distanceM":safe_int(value.get("distance"),(0,i64::MAX)),"redS":red,"straightS":safe_int(value.get(names[0]),(1,999)),"leftS":safe_int(value.get(names[1]),(1,999)),
        "rightS":safe_int(value.get(names[2]),(1,999)),"uturnS":safe_int(value.get(names[3]),(1,999)),"redOn":if detailed{red.is_some()}else{value.get("redLightOn").is_some_and(truth)},
        "straightOn":active(signals[0]),"leftOn":active(signals[1]),"rightOn":active(signals[2]),"uturnOn":active(signals[3])})
}

fn sdi_label(value: Option<&Value>) -> String {
    let Some(kind) = value.and_then(super::packet::integer) else {
        return String::new();
    };
    match kind {
        0 => "Signal speed enforcement",
        1 => "Fixed speed camera",
        2 => "Section control start",
        3 => "Section control end",
        4 => "Section control",
        7 => "Mobile speed camera",
        8 => "Speed camera zone",
        13 => "Traffic data",
        17 => "Parking enforcement",
        20 => "School zone start",
        21 => "School zone end",
        22 => "Speed bump",
        29 => "Accident-prone section",
        30 => "Sharp curve",
        38 => "Frequent speeding",
        63 => "Drowsy rest area",
        84 => "Road caution",
        _ => return format!("SDI type {kind}"),
    }
    .into()
}

pub fn debug(
    value: &Value,
    normalized: Option<&Value>,
    kind: &str,
    timestamp: i64,
    now: f64,
) -> Result<String, Error> {
    let mut title = format!("NAVI {kind}");
    let mut severity = "normal";
    let mut lines = Vec::new();
    let mut speed_limit = None;
    let mut signal = None;
    match (kind, value.get(kind)) {
        ("rgdata", Some(_)) if normalized.is_some() => {
            if let Some(rg) = normalized {
                let sdi = rg.get("nSdiType").and_then(super::packet::integer);
                let plus = rg.get("nSdiPlusType").and_then(super::packet::integer);
                if sdi.is_some_and(|i| matches!(i, 0 | 1 | 2 | 3 | 4 | 7 | 8 | 75 | 76)) {
                    severity = "warning";
                }
                if sdi == Some(22) || plus == Some(22) {
                    severity = "caution";
                }
                let road = rg
                    .get("szPosRoadName")
                    .filter(|v| truth(v))
                    .or_else(|| rg.get("szNearDirName").filter(|v| truth(v)))
                    .map_or(String::new(), py_text);
                let tbt = rg
                    .get("szTBTMainText")
                    .filter(|v| truth(v))
                    .or_else(|| rg.get("szNearDirName").filter(|v| truth(v)))
                    .map_or(String::new(), py_text);
                speed_limit = safe_int(rg.get("nRoadLimitSpeed"), (1, 300));
                lines.extend([
                    line("Road", road),
                    line(
                        "Speed",
                        format!(
                            "{} / limit {} km/h",
                            display(rg, "nPosSpeed"),
                            display(rg, "nRoadLimitSpeed")
                        ),
                    ),
                    line(
                        "TBT",
                        format!(
                            "{tbt}  {}m type {}",
                            display(rg, "nTBTDist"),
                            display(rg, "nTBTTurnType")
                        ),
                    ),
                    line(
                        "SDI",
                        format!(
                            "{}  {}m limit {}",
                            sdi_label(rg.get("nSdiType")),
                            display(rg, "nSdiDist"),
                            display(rg, "nSdiSpeedLimit")
                        ),
                    ),
                ]);
                if plus.is_some_and(|i| !matches!(i, 0 | -1)) {
                    lines.push(line(
                        "SDI+",
                        format!(
                            "{}  {}m",
                            sdi_label(rg.get("nSdiPlusType")),
                            display(rg, "nSdiPlusDist")
                        ),
                    ));
                }
                if rg.get("nLaneCount").is_some_and(|v| !v.is_null())
                    || rg.get("currentLane").is_some_and(|v| !v.is_null())
                {
                    lines.push(line(
                        "Lane",
                        format!(
                            "{}/{} rec {}",
                            display(rg, "currentLane"),
                            display(rg, "nLaneCount"),
                            display(rg, "recommendedLaneNumbers")
                        ),
                    ));
                }
            }
        }
        ("vrtx" | "route", Some(route)) => {
            title = "NAVI route".into();
            let points = route_points(route, 0).unwrap_or_default();
            if let (Some(first), Some(last)) = (points.first(), points.last()) {
                lines.extend([
                    line("Route points", points.len().to_string()),
                    line("First", format!("{:.6}, {:.6}", first.1, first.0)),
                    line("Last", format!("{:.6}, {:.6}", last.1, last.0)),
                ]);
            } else {
                lines.push("Route points: 0".into());
            }
        }
        ("sinf", Some(s)) if s.is_object() => {
            title = "Traffic light".into();
            signal = Some(traffic_debug(s, false));
            if s.get("redLightOn").is_some_and(truth) {
                severity = "stop";
            } else if ["leftLightOn", "greenLightOn"]
                .iter()
                .any(|k| s.get(*k).is_some_and(truth))
            {
                severity = "go";
            }
            lines.extend([
                line("Distance", format!("{}m", display(s, "distance"))),
                line(
                    "Red",
                    format!(
                        "{} {}s",
                        s.get("redLightOn").map_or("None".into(), py_text),
                        display(s, "redLightRemainTime")
                    ),
                ),
                line(
                    "Green",
                    format!(
                        "{} {}s",
                        s.get("greenLightOn").map_or("None".into(), py_text),
                        display(s, "greenLightRemainTime")
                    ),
                ),
                line(
                    "Left",
                    format!(
                        "{} {}s",
                        s.get("leftLightOn").map_or("None".into(), py_text),
                        display(s, "leftLightRemainTime")
                    ),
                ),
            ]);
        }
        ("ssinf", Some(s)) if s.is_object() => {
            title = "Traffic light detail".into();
            signal = Some(traffic_debug(s, true));
            let active = |color| {
                ["straight", "left", "right", "uturn"].iter().any(|k| {
                    s.get(*k)
                        .is_some_and(|v| py_text(v).to_uppercase() == color)
                })
            };
            severity = if active("RED_LIGHT_ON") {
                "stop"
            } else if active("GREEN_LIGHT_ON") {
                "go"
            } else {
                "normal"
            };
            lines.extend([
                line("Distance", format!("{}m", display(s, "distance"))),
                line(
                    "Straight",
                    format!(
                        "{} {}s",
                        display(s, "straight"),
                        display(s, "straight_remain_time")
                    ),
                ),
                line(
                    "Left",
                    format!("{} {}s", display(s, "left"), display(s, "left_remain_time")),
                ),
                line(
                    "Right",
                    format!(
                        "{} {}s",
                        display(s, "right"),
                        display(s, "right_remain_time")
                    ),
                ),
            ]);
        }
        ("complexCrossroad", Some(c)) if c.is_object() => {
            title = "Complex crossroad".into();
            severity = if c.get("show").is_some_and(truth) {
                "caution"
            } else {
                "normal"
            };
            lines.extend([
                line("Show", c.get("show").map_or("None".into(), py_text)),
                line(
                    "Image",
                    format!(
                        "{}x{} {}",
                        display(c, "imageWidth"),
                        display(c, "imageHeight"),
                        text(c, "imageMime")
                    ),
                ),
                line(
                    "Progress",
                    format!(
                        "{}m ratio {}",
                        display(c, "totalMeters"),
                        display(c, "remainRatio")
                    ),
                ),
                line("URL", text(c, "imageUrl")),
            ]);
        }
        _ => {
            lines.push(line(
                "Keys",
                value
                    .as_object()
                    .map(|o| o.keys().take(10).cloned().collect::<Vec<_>>().join(", "))
                    .unwrap_or_default(),
            ));
        }
    }
    Ok(serde_json::to_string(
        &json!({"receivedMono":now,"eventTimeMs":timestamp,"type":kind,"title":title,"severity":severity,"lines":lines.into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>(),"speedLimitKph":speed_limit,"trafficLight":signal}),
    )?)
}
