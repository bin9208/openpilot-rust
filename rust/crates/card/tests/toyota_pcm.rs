use openpilot_card::cruise::toyota_pcm::ToyotaPcmSetSpeedSync;
use openpilot_cereal::car_capnp::{car_params, car_state};
#[test]
fn final_source_pcm_observation_preserves_resume_edges_local_precedence_and_invalidity_reset() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/toyota-pcm-final.json")).unwrap();
    let mut sync = ToyotaPcmSetSpeedSync::default();
    let mut emitted = 0;
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let bytes = |key: &str| {
            row[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b.as_u64().unwrap() as u8)
                .collect::<Vec<_>>()
        };
        let cp = capnp::serialize::read_message(
            bytes("cp").as_slice(),
            capnp::message::ReaderOptions::new(),
        )
        .unwrap();
        let cp = cp.get_root::<car_params::Reader>().unwrap();
        let cs = capnp::serialize::read_message(
            bytes("cs").as_slice(),
            capnp::message::ReaderOptions::new(),
        )
        .unwrap();
        let cs = cs.get_root::<car_state::Reader>().unwrap();
        let eligible = cp.get_brand().unwrap().to_str().unwrap() == "toyota"
            && cp.get_car_fingerprint().unwrap().to_str().unwrap() == "TOYOTA_RAV4_TSS2_2022"
            && cp.get_pcm_cruise()
            && cp.get_openpilot_longitudinal_control()
            && row["mode"] == 0;
        let result = sync
            .update(eligible, cs, row["local"].as_bool().unwrap())
            .unwrap();
        emitted += usize::from(result.is_some());
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            row["result"],
            "result {i}"
        );
        assert_eq!(
            serde_json::to_value(&sync).unwrap(),
            row["state"],
            "state {i}"
        );
    }
    assert!(emitted > 20);
}
