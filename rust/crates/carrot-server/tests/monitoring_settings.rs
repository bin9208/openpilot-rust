use openpilot_carrot_server::{
    param_changes::{History, Paths},
    param_qr::Codec,
    param_restore::Restore,
    params::Backend,
    settings::Catalog,
    Value,
};

#[test]
fn session_state_cannot_be_written_by_web_and_disable_clears_only_offroad_warning() {
    let root = tempfile::tempdir().unwrap();
    let params = openpilot_params::Params::open(&root.path().join("params"), "d").unwrap();
    let mut backend = Backend::native(params, root.path().join("state"));
    for key in [
        "DriverMonitoringSessionDisabled",
        "JetsonLegacyUpdatePending",
    ] {
        assert!(backend.put(key, &Value::Bool(true), None).is_err());
        assert!(Backend::memory(root.path().join("memory"))
            .put(key, &Value::Bool(true), None)
            .is_err());
    }
    for onroad in [true, false] {
        let params = backend.native_params().unwrap();
        params
            .put("IsOnroad", if onroad { b"1" } else { b"0" })
            .unwrap();
        params
            .put("Offroad_DriverMonitoringUncertain", b"warning")
            .unwrap();
        backend
            .put("DriverMonitoringEnabled", &Value::Bool(false), None)
            .unwrap();
        assert_eq!(
            backend
                .native_params()
                .unwrap()
                .get("Offroad_DriverMonitoringUncertain")
                .unwrap()
                .is_some(),
            onroad
        );
    }
}

#[test]
fn backup_and_qr_preserve_schema_but_never_serialize_monitoring_control_state() {
    let root = tempfile::tempdir().unwrap();
    let params = openpilot_params::Params::open(&root.path().join("params"), "d").unwrap();
    let backend = Backend::native(params, root.path().join("state"));
    let backup = backend.backup_values().unwrap();
    let protected = [
        "DriverMonitoringEnabled",
        "DriverMonitoringSessionDisabled",
        "JetsonLegacyUpdatePending",
    ];
    for key in protected {
        assert!(!backup.has(key), "{key}");
    }
    // Historical QR key codes remain decodable; only the exported values change.
    assert!(backend
        .backup_schema()
        .unwrap()
        .0
        .contains(&"DriverMonitoringEnabled".to_owned()));
    let values = Value::object([
        ("DriverMonitoringEnabled", Value::Bool(false)),
        ("DriverMonitoringSessionDisabled", Value::Bool(true)),
        ("JetsonLegacyUpdatePending", Value::Bool(true)),
        ("CarrotVisionEnabled", Value::Bool(true)),
    ]);
    let codec = Codec::new(&backend);
    let encoded = codec.build(&values).unwrap();
    let decoded = codec.parse(encoded.get("payload")).unwrap();
    for key in protected {
        assert!(!decoded.has(key), "{key}");
    }
    assert!(decoded.has("CarrotVisionEnabled"));
}

#[test]
fn restore_ignores_session_state_and_restores_enable_only_for_explicit_default_reset() {
    let root = tempfile::tempdir().unwrap();
    let params = openpilot_params::Params::open(&root.path().join("params"), "d").unwrap();
    params.put("DriverMonitoringEnabled", b"1").unwrap();
    let mut backend = Backend::native(params, root.path().join("state"));
    let catalog = Catalog::from_data(Value::parse(r#"{"params":[]}"#).unwrap()).unwrap();
    let history = History::new(Paths {
        log: root.path().join("history"),
        baseline: root.path().join("baseline"),
    });
    let values = Value::object([
        ("DriverMonitoringEnabled", Value::Bool(false)),
        ("DriverMonitoringSessionDisabled", Value::Bool(true)),
        ("JetsonLegacyUpdatePending", Value::Bool(true)),
        ("CarrotVisionEnabled", Value::Bool(true)),
    ]);
    let mut restore = Restore::new(&mut backend, &catalog, &history);
    let preview = restore.preview(&values, &Value::Null).unwrap();
    assert!(preview.get("summary").get("invalid").number_eq(2));
    assert!(preview.get("summary").get("skipped").number_eq(1));
    assert!(restore
        .apply(&values, &Value::Null, &Value::text("restore"))
        .unwrap()
        .get("result")
        .get("ok_cnt")
        .number_eq(1));
    assert!(backend.get("DriverMonitoringEnabled", &Value::Null).truth());
    let mut restore = Restore::new(&mut backend, &catalog, &history);
    assert!(restore
        .apply(&values, &Value::Null, &Value::text("reset_defaults"))
        .unwrap()
        .get("result")
        .get("ok_cnt")
        .number_eq(1));
    assert!(!backend.get("DriverMonitoringEnabled", &Value::Null).truth());
    for key in [
        "DriverMonitoringSessionDisabled",
        "JetsonLegacyUpdatePending",
    ] {
        assert!(backend.native_params().unwrap().get(key).unwrap().is_none());
    }
}

#[test]
fn search_only_controls_are_searchable_but_absent_from_counts_and_profiles() {
    use openpilot_carrot_server::setting_profiles::ProfileStore;
    let root = tempfile::tempdir().unwrap();
    let catalog = Catalog::from_data(
        Value::parse(
            r#"{"params":[
        {"name":"DriverMonitoringEnabled","group":"DM","default":1,"search_only":true},
        {"name":"DriverMonitoringMode","group":"DM","default":0},
        {"name":"OtherBrand","group":"DM","default":0,"hidden_brands":["hyundai"]}
    ]}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let backend = Backend::memory(root.path().join("state"));
    let profiles = ProfileStore::new(&root.path().join("profiles.json"), &catalog);
    assert!(!profiles
        .snapshot(&backend)
        .unwrap()
        .has("DriverMonitoringEnabled"));
    let cleaned = profiles
        .write(
            &Value::parse(
                r#"{"profiles":[{"id":"one","name":"test","values":{
        "DriverMonitoringEnabled":false,"DriverMonitoringMode":1}}]}"#,
            )
            .unwrap(),
        )
        .unwrap();
    let Value::Array(profiles) = cleaned.get("profiles") else {
        panic!("profiles");
    };
    assert!(!profiles[0].get("values").has("DriverMonitoringEnabled"));
    for (selected, count) in [
        (catalog.clone(), 2),
        (catalog.for_brand("hyundai").unwrap(), 1),
    ] {
        assert!(selected.by_name.has("DriverMonitoringEnabled"));
        let Value::Array(groups) = selected.groups_list else {
            panic!("groups");
        };
        assert!(groups[0].get("count").number_eq(count));
    }
}
