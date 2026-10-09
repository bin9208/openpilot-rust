use super::{compact, Mode, Spec};
use crate::{http::RequestBody, Value};
use hyper::{Request, StatusCode};
const RAW: [&str; 23] = [
    "selfdriveState",
    "carState",
    "controlsState",
    "longitudinalPlan",
    "liveCalibration",
    "modelV2",
    "roadCameraState",
    "deviceState",
    "radarState",
    "carrotMan",
    "gpsLocationExternal",
    "lateralPlan",
    "liveDelay",
    "liveTorqueParameters",
    "liveParameters",
    "navInstructionCarrot",
    "peripheralState",
    "wideRoadCameraState",
    "carControl",
    "liveTracks",
    "cameraOdometry",
    "livePose",
    "carrotNavi",
];
fn strip(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}
pub fn matches(path: &str) -> bool {
    path == "/api/live_runtime"
        || path == "/ws/raw_multiplex"
        || path == "/ws/compact_state"
        || path
            .strip_prefix("/ws/raw/")
            .is_some_and(|name| !name.is_empty() && !name.contains('/'))
        || path
            .strip_prefix("/ws/camera/")
            .is_some_and(|name| !name.is_empty() && !name.contains('/'))
}
pub(super) fn parsed(
    request: &Request<RequestBody>,
    path: &str,
) -> Result<Spec, (StatusCode, String)> {
    let (mode, services, hello) = if let Some(name) = path.strip_prefix("/ws/camera/") {
        let name = strip(name);
        if name != "road" {
            return Err((StatusCode::NOT_FOUND, format!("unknown camera: {name}")));
        }
        (
            Mode::Camera,
            Vec::new(),
            Value::object([
                ("type", Value::text("hello")),
                ("camera", Value::text("road")),
                ("mode", Value::text("direct-encode-relay")),
            ]),
        )
    } else if let Some(name) = path.strip_prefix("/ws/raw/") {
        let name = strip(name);
        if !RAW.contains(&name) {
            return Err((
                StatusCode::NOT_FOUND,
                format!("unknown raw service: {name}"),
            ));
        }
        (
            Mode::Single,
            vec![name.into()],
            Value::object([
                ("type", Value::text("hello")),
                ("service", Value::text(name)),
                ("protocolVersion", Value::integer(1)),
                ("mode", Value::text("raw-capnp-relay")),
                ("wireFormat", Value::text("cereal-event-capnp")),
            ]),
        )
    } else {
        let compact = path == "/ws/compact_state";
        let query = request.uri().query().unwrap_or("");
        let value = url::form_urlencoded::parse(query.as_bytes())
            .find(|(name, _)| name == "services")
            .map(|(_, value)| value.into_owned())
            .unwrap_or_default();
        let requested: Vec<String> = value
            .split(',')
            .map(strip)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if requested.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                if compact {
                    "missing compact state services"
                } else {
                    "missing raw services"
                }
                .into(),
            ));
        }
        let invalid: Vec<_> = requested
            .iter()
            .filter(|name| {
                if compact {
                    !compact::services().any(|allowed| allowed == *name)
                } else {
                    !RAW.contains(&name.as_str())
                }
            })
            .map(String::as_str)
            .collect();
        if !invalid.is_empty() {
            return Err((
                StatusCode::NOT_FOUND,
                format!(
                    "unknown {} services: {}",
                    if compact { "compact state" } else { "raw" },
                    invalid.join(",")
                ),
            ));
        }
        let mut unique = Vec::new();
        for name in &requested {
            if !unique.contains(name) {
                unique.push(name.clone());
            }
        }
        let hello = if compact {
            Value::object([
                ("mode", Value::text("carrot-state-v1")),
                (
                    "services",
                    Value::Array(unique.iter().map(|name| Value::text(name)).collect()),
                ),
            ])
        } else {
            Value::object([
                ("type", Value::text("hello")),
                (
                    "services",
                    Value::Array(requested.iter().map(|name| Value::text(name)).collect()),
                ),
                ("protocolVersion", Value::integer(1)),
                ("mode", Value::text("raw-capnp-multiplex-relay")),
                ("wireFormat", Value::text("service-name+capnp-frame")),
            ])
        };
        (
            if compact {
                Mode::Compact
            } else {
                Mode::Multiplex
            },
            unique,
            hello,
        )
    };
    let hello = hello
        .encode()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Spec {
        mode,
        services,
        hello: crate::state_json::compact_encoded(&hello),
    })
}
