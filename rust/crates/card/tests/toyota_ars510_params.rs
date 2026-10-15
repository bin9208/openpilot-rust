use openpilot_card::{
    brands::toyota::{self, ParamsInput},
    firmware::Firmware,
};
use openpilot_cereal::car_capnp::car_params;
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Deserialize)]
struct Request {
    candidate: String,
    alpha_long: bool,
    fingerprints: Vec<(u8, Vec<(u32, usize)>)>,
    firmware: Vec<Firmware>,
    settings: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct Row {
    request: Request,
    params: Vec<u8>,
}
#[test]
fn source_ars510_capability_changes_only_radar_declaration_for_valid_vehicle_configs() {
    let rows: Vec<Row> = serde_json::from_slice(
        &zstd::stream::decode_all(
            &include_bytes!("fixtures/toyota-ars510-params-final.json.zst")[..],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(rows.len(), 32);
    for (index, row) in rows.into_iter().enumerate() {
        let directory = tempfile::tempdir().unwrap();
        let settings = openpilot_params::Params::open(directory.path(), "d").unwrap();
        for (key, value) in row.request.settings {
            settings.put(&key, value.as_bytes()).unwrap();
        }
        let native = toyota::parameters(ParamsInput {
            candidate: &row.request.candidate,
            fingerprints: &row.request.fingerprints,
            firmware: &row.request.firmware,
            alpha_long: row.request.alpha_long,
            settings: &settings,
        })
        .unwrap();
        let actual = native
            .get_root_as_reader::<car_params::Reader<'_>>()
            .unwrap();
        let message =
            capnp::serialize::read_message(&row.params[..], capnp::message::ReaderOptions::new())
                .unwrap();
        let expected = message.get_root::<car_params::Reader<'_>>().unwrap();
        assert_eq!(
            actual.get_radar_unavailable(),
            expected.get_radar_unavailable(),
            "radar availability case {index}"
        );
        assert_eq!(
            actual.get_radar_time_step().to_bits(),
            expected.get_radar_time_step().to_bits(),
            "period case {index}"
        );
        assert_eq!(
            actual.get_radar_delay().to_bits(),
            expected.get_radar_delay().to_bits(),
            "delay case {index}"
        );
        assert_eq!(
            actual.get_flags(),
            expected.get_flags(),
            "flags case {index}"
        );
        assert_eq!(
            actual.get_openpilot_longitudinal_control(),
            expected.get_openpilot_longitudinal_control(),
            "longitudinal case {index}"
        );
        assert_eq!(
            actual.get_alpha_longitudinal_available(),
            expected.get_alpha_longitudinal_available(),
            "availability case {index}"
        );
        let (a, b) = (
            actual.get_safety_configs().unwrap(),
            expected.get_safety_configs().unwrap(),
        );
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.get_safety_model().unwrap(), b.get_safety_model().unwrap());
            assert_eq!(a.get_safety_param(), b.get_safety_param());
        }
    }
}
