use crate::{json::Value, Error};
use num_bigint::BigInt;
use num_traits::ToPrimitive;

pub const CATALOG: &[(&str, &str)] = &[
    ("json", "vehicle"),
    ("json", "guidance_current"),
    ("json", "guidance_next"),
    ("json", "lane_current"),
    ("json", "lane_ahead"),
    ("json", "speed"),
    ("json", "traffic_signal"),
    ("json", "crossroad"),
    ("json", "route"),
    ("json", "navigation_status"),
    ("json", "app_status"),
    ("json", "camera_state"),
    ("json", "composition_state"),
    ("image", "tbt_current_compact"),
    ("image", "tbt_current_full"),
    ("image", "tbt_next"),
    ("image", "traffic_signal"),
    ("image", "lane_top"),
    ("image", "lane_bottom"),
    ("image", "safety_primary"),
    ("image", "safety_secondary"),
    ("image", "safety_section"),
    ("image", "crossroad_minimized"),
    ("image", "crossroad_expanded"),
    ("image", "center_tbt_icon"),
    ("image", "center_tbt_text"),
    ("image", "center_tbt_fee"),
    ("render", "map_main"),
];
pub const CLUSTER_NAMES: &[&str] = &[
    "vehicle",
    "guidance_current",
    "guidance_next",
    "lane_current",
    "lane_ahead",
    "speed",
    "traffic_signal",
    "crossroad",
    "route",
    "navigation_status",
];

#[derive(Debug, Clone, PartialEq)]
pub struct MapConfig {
    pub theme: String,
    pub map_type: String,
    pub hz: u32,
    pub bitrate_kbps: u32,
    pub screen_center_y_ratio: f64,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            theme: "auto".into(),
            map_type: "normal".into(),
            hz: 10,
            bitrate_kbps: 3000,
            screen_center_y_ratio: 0.8,
        }
    }
}

fn supplied(fields: &Value, key: &str, default: Value) -> Value {
    if fields.has(key) {
        fields.get(key).clone()
    } else {
        default
    }
}

fn normalized(value: &Value, error_prefix: &str) -> Result<String, Error> {
    let text = match value.string() {
        Ok(text) => text,
        Err(error) if error.kind == "UnicodeEncodeError" => {
            return Err(Error::value_detail(error_prefix, value)?)
        }
        Err(error) => return Err(error),
    };
    Ok(text
        .trim_matches(|character: char| {
            character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
        })
        .to_lowercase())
}

impl MapConfig {
    pub fn parse(fields: &Value) -> Result<Self, Error> {
        let raw_theme = supplied(fields, "map_theme", Value::text("auto"));
        let theme = normalized(&raw_theme, "unsupported map theme: ")?;
        if !["auto", "dark", "light"].contains(&theme.as_str()) {
            return Err(Error::value(&format!(
                "unsupported map theme: {}",
                raw_theme.string()?
            )));
        }
        let raw_type = supplied(fields, "map_type", Value::text("normal"));
        let map_type = normalized(&raw_type, "unsupported map type: ")?;
        if !["normal", "satellite"].contains(&map_type.as_str()) {
            return Err(Error::value(&format!(
                "unsupported map type: {}",
                raw_type.string()?
            )));
        }
        let raw_hz = supplied(fields, "map_hz", Value::integer(10));
        let hz = raw_hz.int()?;
        if hz < BigInt::from(1) || hz > BigInt::from(60) {
            return Err(Error::value(&format!(
                "unsupported map refresh rate: {}",
                raw_hz.string()?
            )));
        }
        let raw_bitrate = supplied(fields, "map_bitrate_kbps", Value::integer(3000));
        let bitrate = raw_bitrate.int()?;
        if bitrate < BigInt::from(1) || bitrate > BigInt::from(12000) {
            return Err(Error::value(&format!(
                "unsupported map bitrate: {}",
                raw_bitrate.string()?
            )));
        }
        let raw_ratio = supplied(fields, "screen_center_y_ratio", Value::Float(0.8));
        let ratio = raw_ratio.float()?;
        if !(0.5..=0.9).contains(&ratio) {
            return Err(Error::value(&format!(
                "unsupported map screen center ratio: {}",
                raw_ratio.string()?
            )));
        }
        Ok(Self {
            theme,
            map_type,
            hz: hz
                .to_u32()
                .ok_or_else(|| Error::value("validated refresh rate overflow"))?,
            bitrate_kbps: bitrate
                .to_u32()
                .ok_or_else(|| Error::value("validated bitrate overflow"))?,
            screen_center_y_ratio: ratio,
        })
    }
    pub fn manifest(&self, session: &str, revision: Value) -> Value {
        let streams = CATALOG
            .iter()
            .enumerate()
            .map(|(index, &(kind, name))| {
                Value::object([
                    ("kind", Value::text(kind)),
                    ("name", Value::text(name)),
                    ("schema_version", Value::integer(1)),
                    ("stream_handle", Value::integer(index + 1)),
                    (
                        "enabled",
                        Value::Bool(kind != "image" || name != "lane_top"),
                    ),
                    ("params", self.stream_params(kind, name)),
                ])
            })
            .collect();
        Value::object([
            ("type", Value::text("subscription_manifest")),
            ("protocol_version", Value::integer(2)),
            ("session_id", Value::text(session)),
            ("revision", revision),
            ("metrics_enabled", Value::Bool(false)),
            ("limit_adjustments", Value::Array(Vec::new())),
            ("streams", Value::Array(streams)),
        ])
    }
    fn stream_params(&self, kind: &str, name: &str) -> Value {
        if kind == "json" {
            return Value::object([
                ("delivery_mode", Value::text("on_change")),
                ("interval_ms", Value::integer(1000)),
                ("stale_timeout_ms", Value::integer(10000)),
            ]);
        }
        if kind == "image" {
            let mut fields = vec![
                ("format", Value::text("png")),
                ("max_fps", Value::integer(5)),
                ("stale_timeout_ms", Value::integer(15000)),
            ];
            if name.starts_with("crossroad_") {
                fields.push(("theme", Value::text(&self.theme)));
            }
            return Value::Object(
                fields
                    .into_iter()
                    .map(|(key, value)| (key.chars().map(u32::from).collect(), value))
                    .collect(),
            );
        }
        Value::object([
            ("composition", Value::text("map_route_vehicle")),
            ("width", Value::integer(960)),
            ("height", Value::integer(540)),
            ("dpi", Value::integer(360)),
            ("fps", Value::integer(self.hz)),
            ("jpeg_quality", Value::integer(75)),
            ("codec", Value::text("h264")),
            ("h264_bitrate_kbps", Value::integer(self.bitrate_kbps)),
            ("h264_keyframe_interval_sec", Value::integer(2)),
            ("camera_mode", Value::text("app_sync")),
            ("map_theme", Value::text(&self.theme)),
            ("map_type", Value::text(&self.map_type)),
            ("zoom", Value::Float(11.)),
            ("tilt", Value::Float(50.)),
            ("bearing", Value::Float(0.)),
            ("follow_vehicle_bearing", Value::Bool(true)),
            ("fov", Value::Float(40.)),
            (
                "screen_center_y_ratio",
                Value::Float(self.screen_center_y_ratio),
            ),
            ("follow_vehicle", Value::Bool(true)),
            ("center_latitude", Value::Null),
            ("center_longitude", Value::Null),
            ("stale_timeout_ms", Value::integer(5000)),
        ])
    }
}

pub fn resolve_hz(mode: &Value) -> Result<u32, Error> {
    let mode = match mode.int() {
        Ok(mode) => mode,
        Err(error) if matches!(error.kind, "TypeError" | "ValueError") => BigInt::from(1),
        Err(error) => return Err(error),
    };
    Ok(match mode.to_i32() {
        Some(0) => 5,
        Some(1) => 10,
        Some(2) => 20,
        Some(3) => 30,
        _ => 10,
    })
}

pub fn resolve_bitrate(width: &Value, height: &Value, hz: &Value) -> Result<BigInt, Error> {
    let pixels = width.int()?.max(BigInt::from(1)) * height.int()?.max(BigInt::from(1));
    let hz = hz.int()?.clamp(BigInt::from(1), BigInt::from(60));
    let reference = match hz.to_u32() {
        Some(5) => BigInt::from(1500),
        Some(10 | 20) => BigInt::from(3000),
        Some(30) => BigInt::from(6000),
        _ => (BigInt::from(3000) * hz + BigInt::from(5)) / BigInt::from(10),
    };
    Ok(
        ((reference * pixels + BigInt::from(259200)) / BigInt::from(518400))
            .clamp(BigInt::from(1), BigInt::from(12000)),
    )
}
