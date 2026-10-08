use super::{http, options::Options, session::Route, wire};
use crate::{json::Value, manifest::MapConfig, receiver::Receiver};
use openpilot_cereal::log_capnp::event;

#[test]
fn wall_milliseconds_floor_integer_nanoseconds_near_second_boundary() {
    let time = rustix::time::Timespec {
        tv_sec: 1_700_000_000,
        tv_nsec: 999_999_999,
    };
    assert_eq!(super::clock::wall_millis(time), 1_700_000_000_999);
}

#[test]
fn absent_upgrade_header_keeps_original_http_error_text() {
    assert_eq!(
        http::handshake_error(&hyper::HeaderMap::new()).unwrap(),
        "No WebSocket UPGRADE hdr: None\n Can \"Upgrade\" only to \"WebSocket\"."
    );
}

#[test]
fn encoded_route_parameters_preserve_decoded_slashes_and_unicode() {
    let Some(Route::Item {
        kind,
        session,
        name,
    }) = http::route("/api/navi/ws/v2/json/a%2Fb/%ED%95%9C%EA%B8%80")
    else {
        panic!("route rejected");
    };
    assert_eq!(
        (kind, session.as_str(), name.as_str()),
        ("json", "a/b", "한글")
    );
    let Some(Route::Control(version)) = http::route("/api/navi/ws/v2/control/%é") else {
        panic!("route rejected");
    };
    assert_eq!(version, "%é");
}

#[test]
fn default_options_and_advertisement_match_registered_main() {
    let options = Options::parse(Vec::new()).unwrap().unwrap();
    assert_eq!(
        (
            options.host.as_str(),
            options.port,
            options.beacon,
            options.cereal
        ),
        ("0.0.0.0", 7714, true, true)
    );
    assert_eq!(options.advertised(), None);
    let options = Options::parse(["--host".into(), "127.0.0.1".into(), "--no-cereal".into()])
        .unwrap()
        .unwrap();
    assert_eq!(options.advertised(), Some("127.0.0.1"));
    assert!(!options.cereal);
}

#[test]
fn state_wire_contains_full_empty_projection_and_valid_event() {
    let receiver = Receiver::new(Value::integer(7714), MapConfig::default());
    let bytes = wire::state(&receiver.cereal_snapshot()).unwrap();
    let message =
        capnp::serialize::read_message(&mut bytes.as_slice(), capnp::message::ReaderOptions::new())
            .unwrap();
    let event = message.get_root::<event::Reader<'_>>().unwrap();
    assert!(event.get_valid());
    assert!(event.get_log_mono_time() > 0);
    let event::Which::CarrotNavi(body) = event.which().unwrap() else {
        panic!("wrong event union");
    };
    let body = body.unwrap();
    assert_eq!(body.get_schema_version(), 1);
    assert_eq!(body.get_generation(), 0);
    assert!(!body.get_connected());
    assert_eq!(body.get_guidance_current().unwrap().get_turn_type(), -1);
    assert_eq!(body.get_lane_current().unwrap().get_current_lane(), -1);
    assert_eq!(body.get_speed().unwrap().get_sdi_type(), -1);
    assert_eq!(body.get_lane_ahead().unwrap().len(), 0);
    assert_eq!(body.get_route().unwrap().get_polyline().unwrap().len(), 0);
}
