use openpilot_card::brands::hyundai::{Hyundai, ParamsInput};
#[test]
fn pinned_direct_cluster_opt_in_sets_exact_platform_and_panda_safety_masks() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/direct-cluster-final.json")).unwrap();
    let path = std::env::temp_dir().join(format!("native-direct-cluster-{}", std::process::id()));
    let settings = openpilot_params::Params::open(&path, "p").unwrap();
    for row in fixture["rows"].as_array().unwrap() {
        settings
            .put(
                "HyundaiCameraSCC",
                row["camera"].as_i64().unwrap().to_string().as_bytes(),
            )
            .unwrap();
        settings.put("CanfdHDA2", b"0").unwrap();
        settings.put("EnableRadarTracks", b"0").unwrap();
        settings
            .put_bool(
                "HyundaiCanfdClusterDirectTx",
                row["direct"].as_bool().unwrap(),
            )
            .unwrap();
        let result = Hyundai::parameters(ParamsInput {
            candidate: row["candidate"].as_str().unwrap(),
            fingerprints: &[],
            firmware: &[],
            alpha_long: true,
            is_release: false,
            settings: &settings,
        })
        .unwrap();
        let cp = result
            .get_root_as_reader::<openpilot_cereal::car_capnp::car_params::Reader<'_>>()
            .unwrap();
        assert_eq!(
            u64::from(cp.get_flags()),
            row["flags"].as_u64().unwrap(),
            "{row}"
        );
        let safety:Vec<_>=cp.get_safety_configs().unwrap().iter().map(|s|serde_json::json!({"model":u16::from(s.get_safety_model().unwrap()),"parameter":s.get_safety_param()})).collect();
        assert_eq!(serde_json::json!(safety), row["safety"], "{row}");
    }
    std::fs::remove_dir_all(path).unwrap();
}
