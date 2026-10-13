use super::{clock, key::Key, network::Endpoint, profiles, state::State};
use crate::Value;

fn named(value: &Value, name: &str, additional: &[(&str, Value)]) -> Value {
    let mut result = value.clone();
    let _name = crate::json_fields::set(&mut result, "name", Value::text(name));
    for (name, value) in additional {
        let _field = crate::json_fields::set(&mut result, name, value.clone());
    }
    result
}
pub(super) fn diagnostics(state: &mut State, endpoint: &Endpoint) -> Value {
    let now = clock::now();
    let status = super::status::status(state, now);
    let key = state.keys.get();
    let profile = state.profile(now.mono);
    Value::object([
        ("generated_at", Value::Float(clock::now().wall)),
        ("status", status),
        (
            "config",
            Value::object([
                ("stream_key_configured", Value::Bool(key.configured())),
                ("stream_key_masked", key.mask()),
                ("source", Value::text(profiles::SOURCE)),
                ("quality", Value::text(profile.label)),
                ("target", profile.target()),
                (
                    "rtmps_ingest",
                    if key.configured() {
                        key.ingest(&endpoint.base)
                    } else {
                        Value::text("")
                    },
                ),
            ]),
        ),
        (
            "params",
            Value::object([
                ("CarrotYouTubeLive", Value::Bool(state.enabled(now.mono))),
                (
                    "CarrotYouTubeQuality",
                    Value::integer(state.settings.integer("CarrotYouTubeQuality", now.mono)),
                ),
                (
                    "CarrotYouTubeTimestamp",
                    Value::Bool(state.settings.boolean("CarrotYouTubeTimestamp", now.mono)),
                ),
                (
                    "ClusterHud",
                    Value::integer(state.settings.integer("ClusterHud", now.mono)),
                ),
                (
                    "DisableDM",
                    Value::integer(state.settings.integer("DisableDM", now.mono)),
                ),
                (
                    "IsOnroad",
                    Value::Bool(state.settings.boolean("IsOnroad", now.mono)),
                ),
            ]),
        ),
        (
            "transport",
            named(
                &state.capabilities.transport,
                "librtmp-rtmps",
                &[
                    ("connected", Value::Bool(state.connected)),
                    ("log_tail", state.log_tail()),
                ],
            ),
        ),
        (
            "muxer",
            named(&state.capabilities.muxer, "flv-h264-aac", &[]),
        ),
        ("processes", state.resources.processes(now.mono)),
        (
            "state_path",
            Value::text(&state.state_path.to_string_lossy()),
        ),
    ])
}
pub(super) fn test(state: &mut State, observation: (Key, (bool, String))) -> Value {
    let (key, reachable) = observation;
    let now = clock::now();
    let profile = state.profile(now.mono);
    let status = super::status::status(state, now);
    let transport = state.capabilities.transport.get("available").truth();
    let muxer = state.capabilities.ready
        || (state.capabilities.muxer.get("available").truth()
            && state.capabilities.muxer.get("h264").truth()
            && state.capabilities.muxer.get("aac").truth());
    let ok = key.configured() && transport && muxer && reachable.0;
    Value::object([
        ("ok", Value::Bool(ok)),
        ("configured", Value::Bool(key.configured())),
        ("transport_available", Value::Bool(transport)),
        ("transport", state.capabilities.transport.clone()),
        ("rtmps_reachable", Value::Bool(reachable.0)),
        ("rtmps_message", Value::text(&reachable.1)),
        ("muxer_available", Value::Bool(muxer)),
        ("muxer", state.capabilities.muxer.clone()),
        ("source", Value::text(profiles::SOURCE)),
        ("quality", Value::text(profile.label)),
        ("target", profile.target()),
        ("resource_status", status.get("resource_status").clone()),
        ("warnings", status.get("warnings").clone()),
        ("log_tail", state.log_tail()),
        (
            "message",
            Value::text(if ok {
                "ready"
            } else {
                "stream key, RTMPS network, librtmp, or FLV/AAC muxer is unavailable"
            }),
        ),
    ])
}
pub(super) fn validation(
    state: &mut State,
    key: &Key,
    observation: ((bool, &str), (bool, String)),
) -> Value {
    let (format, reachable) = observation;
    let transport = state.capabilities.transport.get("available").truth();
    let muxer = state.capabilities.muxer.get("available").truth()
        && state.capabilities.muxer.get("h264").truth()
        && state.capabilities.muxer.get("aac").truth();
    Value::object([
        ("ok", Value::Bool(format.0 && reachable.0 && transport && muxer)), ("configured", Value::Bool(state.keys.get().configured())),
        ("format_ok", Value::Bool(format.0)), ("format_message", Value::text(format.1)),
        ("rtmps_reachable", Value::Bool(reachable.0)), ("rtmps_message", Value::text(&reachable.1)),
        ("transport_available", Value::Bool(transport)), ("muxer_available", Value::Bool(muxer)),
        ("masked_key", key.mask()), ("note", Value::text("YouTube only confirms whether the key is accepted when an encoder starts streaming.")),
    ])
}
