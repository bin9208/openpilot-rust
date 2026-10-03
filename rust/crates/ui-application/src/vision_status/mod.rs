mod encoding;
use openpilot_logmessaged::{JsonValue, JsonView};
use serde::Serialize;

pub const LANE_TIMEOUT_NS: i128 = 4_000_000_000;
pub const BLINDSPOT_TIMEOUT_NS: i128 = 1_500_000_000;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("vision packet encoding")]
    Encoding,
    #[error(transparent)]
    Json(#[from] openpilot_logmessaged::JsonError),
    #[error("vision packet field {0}")]
    Field(&'static str),
    #[error("vision latency integer exceeds float range")]
    LatencyOverflow,
}
#[derive(Clone, Debug)]
pub struct Nanoseconds(String);
impl Nanoseconds {
    pub fn decimal(&self) -> &str {
        &self.0
    }
    fn fresh(&self, now: i128, timeout: i128) -> bool {
        self.0.parse::<i128>().ok().is_some_and(|received| {
            received > 0
                && now
                    .checked_sub(received)
                    .is_some_and(|age| (0..=timeout).contains(&age))
        })
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum Latency {
    Integer(String),
    Float(f64),
}
impl Latency {
    pub fn seconds(&self) -> Result<f64, Error> {
        match self {
            Self::Float(value) => Ok(value / 1000.0),
            Self::Integer(value) => {
                let value = if value.len() > 3 {
                    let (whole, fraction) = value.split_at(value.len() - 3);
                    format!("{whole}.{fraction}")
                } else {
                    format!("0.{value:0>3}")
                };
                value.parse().map_err(|_| Error::LatencyOverflow)
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct VisionResult {
    pub left_lane: i32,
    pub right_lane: i32,
    pub lane_valid: bool,
    pub lane_received_nanos: Nanoseconds,
    pub left_blindspot: bool,
    pub right_blindspot: bool,
    pub blindspot_valid: bool,
    pub blindspot_received_nanos: Nanoseconds,
}
#[derive(Clone, Debug)]
pub struct Packet {
    pub result: VisionResult,
    pub blindspot_side: String,
    pub latency_ms: Option<Latency>,
}
fn field(object: &JsonValue, name: &'static str) -> Result<JsonValue, Error> {
    object.get(name).ok_or(Error::Field(name))
}
fn lane(value: JsonValue) -> Result<i32, Error> {
    match value.view() {
        JsonView::Integer(value) => match value {
            "-1" => Ok(-1),
            "0" => Ok(0),
            "1" => Ok(1),
            _ => Err(Error::Field("lane type")),
        },
        _ => Err(Error::Field("lane type")),
    }
}
fn boolean(value: JsonValue) -> Result<bool, Error> {
    match value.view() {
        JsonView::Bool(value) => Ok(value),
        _ => Err(Error::Field("boolean")),
    }
}
fn timestamp(value: JsonValue) -> Result<Nanoseconds, Error> {
    match value.view() {
        JsonView::Integer(value) if !value.starts_with('-') => Ok(Nanoseconds(value.into())),
        _ => Err(Error::Field("receivedMonoTimeNanos")),
    }
}
fn latency(value: Option<JsonValue>) -> Result<Option<Latency>, Error> {
    let Some(value) = value else {
        return Ok(None);
    };
    Ok(match value.view() {
        JsonView::Integer(decimal) => {
            let value = decimal.parse::<f64>().map_err(|_| Error::LatencyOverflow)?;
            if !value.is_finite() {
                return Err(Error::LatencyOverflow);
            }
            (value >= 0.0).then(|| Latency::Integer(decimal.into()))
        }
        JsonView::Float(value) if value.is_finite() && value >= 0.0 => Some(Latency::Float(value)),
        _ => None,
    })
}
impl Packet {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let source = encoding::decode(payload)?;
        let data = JsonValue::parse(&source)?;
        let version = data.get("version").is_some_and(|value| match value.view() {
            JsonView::Integer(value) => value == "1",
            JsonView::Float(value) => value == 1.0,
            JsonView::Bool(value) => value,
            _ => false,
        });
        if !data.is_object()
            || !data
                .get("type")
                .is_some_and(|value| value.text_eq("xiaogeVision"))
            || !version
        {
            return Err(Error::Field("version 1 xiaogeVision object"));
        }
        let lanes = field(&data, "lane")?;
        let blindspot = field(&data, "blindspot")?;
        if !lanes.is_object() || !blindspot.is_object() {
            return Err(Error::Field("lane/blindspot object"));
        }
        let result = VisionResult {
            left_lane: lane(field(&lanes, "leftLine")?)?,
            right_lane: lane(field(&lanes, "rightLine")?)?,
            lane_valid: boolean(field(&lanes, "valid")?)?,
            lane_received_nanos: timestamp(field(&lanes, "receivedMonoTimeNanos")?)?,
            left_blindspot: boolean(field(&blindspot, "left")?)?,
            right_blindspot: boolean(field(&blindspot, "right")?)?,
            blindspot_valid: boolean(field(&blindspot, "valid")?)?,
            blindspot_received_nanos: timestamp(field(&blindspot, "receivedMonoTimeNanos")?)?,
        };
        let side = blindspot.get("side");
        let side = match side {
            Some(value) if value.text_eq("left") => "left",
            Some(value) if value.text_eq("right") => "right",
            _ => "",
        };
        Ok(Self {
            result,
            blindspot_side: side.into(),
            latency_ms: latency(lanes.get("latencyMs"))?,
        })
    }
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Waiting,
    Running,
    Stale,
}
#[derive(Clone, Debug, Serialize)]
pub struct DisplayState {
    pub state: Status,
    pub left_lane: i32,
    pub right_lane: i32,
    pub clear_side: String,
    pub latency_ms: Option<Latency>,
}
impl DisplayState {
    pub fn at(packet: Option<&Packet>, now: i128) -> Self {
        let Some(packet) = packet else {
            return Self {
                state: Status::Waiting,
                left_lane: -1,
                right_lane: -1,
                clear_side: String::new(),
                latency_ms: None,
            };
        };
        let result = &packet.result;
        let lane_fresh =
            result.lane_valid && result.lane_received_nanos.fresh(now, LANE_TIMEOUT_NS);
        let blindspot_fresh = result.blindspot_valid
            && result
                .blindspot_received_nanos
                .fresh(now, BLINDSPOT_TIMEOUT_NS);
        let detected = if packet.blindspot_side == "left" {
            result.left_blindspot
        } else {
            result.right_blindspot
        };
        Self {
            state: if lane_fresh {
                Status::Running
            } else {
                Status::Stale
            },
            left_lane: if lane_fresh { result.left_lane } else { -1 },
            right_lane: if lane_fresh { result.right_lane } else { -1 },
            clear_side: if blindspot_fresh && !detected {
                packet.blindspot_side.clone()
            } else {
                String::new()
            },
            latency_ms: if lane_fresh {
                packet.latency_ms.clone()
            } else {
                None
            },
        }
    }
}
