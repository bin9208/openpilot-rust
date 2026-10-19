use openpilot_jetlink::{
    model_spec::{ModelSpec, LEGACY_JSON},
    wire::Version,
};

#[test]
fn driving_graph_contracts_match_pinned_source_vectors() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("model_spec_vectors.json")).unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let valid = ModelSpec::parse(&serde_json::to_vec(&case["spec"]).unwrap()).is_ok();
        assert_eq!(
            valid,
            case["valid"].as_bool().unwrap(),
            "case {index}: {}",
            case["spec"]
        );
    }
}

#[test]
fn wire_version_is_independent_of_model_generation_and_recurrence() {
    let queued = ModelSpec::parse(LEGACY_JSON.as_bytes()).unwrap();
    assert!(!queued.stateful());
    assert_eq!(queued.packed_count(Version::V2).unwrap(), 16396);
    assert_eq!(queued.packed_count(Version::V3).unwrap(), 12);
    let stateful = ModelSpec::parse(openpilot_jetlink::contract::CONTRACT_JSON.as_bytes()).unwrap();
    assert!(stateful.stateful());
    assert!(stateful.packed_count(Version::V2).is_err());
    assert_eq!(stateful.packed_count(Version::V3).unwrap(), 12);
    assert_ne!(queued.sha256, stateful.sha256);
}

#[test]
fn selected_session_adapter_checks_exact_output_and_keeps_native_model() {
    use openpilot_jetlink::{adapter::Adapter, contract, Deadline};
    use std::time::Duration;
    for bytes in [LEGACY_JSON.as_bytes(), contract::CONTRACT_JSON.as_bytes()] {
        let spec = ModelSpec::parse(bytes).unwrap();
        let mut adapter = Adapter::for_spec(&spec).unwrap();
        let mut output = vec![0.; spec.output_count().unwrap()];
        let deadline = || Deadline::after(Duration::from_secs(1)).unwrap();
        assert!(adapter.parse(&output, deadline()).is_ok());
        assert!(!adapter.reset_next());
        adapter.reset();
        output[0] = f32::NAN;
        assert!(adapter.parse(&output, deadline()).is_err());
        assert!(adapter.reset_next());
        assert!(adapter
            .parse(&output[..output.len() - 1], deadline())
            .is_err());
    }
}

#[test]
fn app_selection_is_exact_onroad_and_fail_closed_on_corruption() {
    use openpilot_jetlink::selection;
    let cache = tempfile::tempdir().unwrap();
    let spec = ModelSpec::parse(LEGACY_JSON.as_bytes()).unwrap();
    assert!(selection::approved(cache.path()).unwrap().is_none());
    selection::remember(cache.path(), &spec).unwrap();
    selection::remember(cache.path(), &spec).unwrap();
    let mut changed_contract = spec.clone();
    changed_contract.checkpoint = Some("changed".into());
    assert!(selection::remember(cache.path(), &changed_contract).is_err());
    let mut changed = spec.clone();
    changed.sha256 = "a".repeat(64);
    selection::remember(cache.path(), &changed).unwrap();
    assert_eq!(selection::approved(cache.path()).unwrap().unwrap(), changed);
    assert!(selection::check_loaded(&spec, &serde_json::json!({"loaded":changed.sha256})).is_err());
    assert!(selection::check_loaded(&spec, &serde_json::json!({"loaded":null})).is_err());
    assert!(selection::check_loaded(&spec, &serde_json::json!({"loaded":spec.sha256})).is_ok());
    std::fs::write(cache.path().join("selected.json"), b"invalid").unwrap();
    assert!(selection::approved(cache.path()).is_err());
    assert!(selection::remember(cache.path(), &spec).is_err());
}

#[test]
fn only_failed_hello_changes_automatic_protocol_on_a_new_stream() {
    use openpilot_jetlink::protocol_choice::ProtocolChoice;
    let mut automatic = ProtocolChoice::parse("auto").unwrap();
    assert_eq!(automatic.version, Version::V3);
    automatic.handshake_failed();
    assert_eq!(automatic.version, Version::V2);
    automatic.handshake_failed();
    assert_eq!(automatic.version, Version::V3);
    for setting in ["2", "3"] {
        let mut pinned = ProtocolChoice::parse(setting).unwrap();
        let before = pinned.version;
        pinned.handshake_failed();
        assert_eq!(pinned.version, before);
    }
    assert!(ProtocolChoice::parse("").is_err());
}
