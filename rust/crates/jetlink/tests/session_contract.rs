use openpilot_jetlink::{
    contract, model_spec::LEGACY_JSON, session_contract::SessionContract, wire::Version,
};
#[test]
fn source_cinque_generation_and_wire_version_are_independent() {
    let raw = serde_json::from_str(LEGACY_JSON).unwrap();
    let v2 = SessionContract::parse(raw, Version::V2).unwrap();
    assert_eq!(v2.packed_count(), 16396);
    let v3 =
        SessionContract::parse(serde_json::from_str(LEGACY_JSON).unwrap(), Version::V3).unwrap();
    assert_eq!(v3.packed_count(), 12);
    assert_eq!(v2.spec, v3.spec);
    assert_ne!(v3.spec.sha256, contract::SHA256);
}
#[test]
fn only_exact_validated_native_contract_keeps_historic_v2_exception() {
    assert_eq!(SessionContract::native_legacy().unwrap().packed_count(), 12);
    let mut raw = contract::contract().unwrap();
    raw["sha256"] = serde_json::json!("a".repeat(64));
    assert!(SessionContract::parse(raw.clone(), Version::V2).is_err());
    assert!(SessionContract::parse(raw, Version::V3).is_ok());
}
#[test]
fn queued_v2_feedback_and_v3_server_history_use_the_selected_graph() {
    use openpilot_jetlink::{adapter::Adapter, Deadline};
    use std::time::Duration;
    let session =
        SessionContract::parse(serde_json::from_str(LEGACY_JSON).unwrap(), Version::V2).unwrap();
    let mut adapter = Adapter::for_spec(&session.spec).unwrap();
    let warped = vec![0; contract::WARPED_BYTES];
    let packed = adapter
        .prepare_session(&warped, [0.; 8], [0.; 2], [0.; 2], session.packed_count())
        .unwrap();
    assert!(packed.iter().all(|v| *v == 0.));
    let [start, stop] = session.spec.range("hidden_state").unwrap();
    let mut output = vec![0.; session.spec.output_count().unwrap()];
    for (i, v) in output[start..stop].iter_mut().enumerate() {
        *v = i as f32 / 16384.;
    }
    adapter
        .parse(&output, Deadline::after(Duration::from_millis(50)).unwrap())
        .unwrap();
    let packed = adapter
        .prepare_session(&warped, [0.; 8], [0.; 2], [0.; 2], session.packed_count())
        .unwrap();
    assert_eq!(&packed[12..], &output[start..stop]);
    assert_eq!(
        adapter
            .prepare_session(&warped, [0.; 8], [0.; 2], [0.; 2], 12)
            .unwrap()
            .len(),
        12
    );
    adapter.reset();
    assert!(adapter
        .prepare_session(&warped, [0.; 8], [0.; 2], [0.; 2], session.packed_count())
        .unwrap()
        .iter()
        .all(|v| *v == 0.));
}
