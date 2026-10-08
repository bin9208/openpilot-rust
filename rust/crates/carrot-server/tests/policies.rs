use openpilot_carrot_server::{
    config::Config,
    params::{coerce_inferred, Backend},
    settings::{Catalog, SettingsCache},
    Value,
};
use std::{
    fs,
    time::{Duration, UNIX_EPOCH},
};

#[test]
fn gap_brand_and_group_order_do_not_change_cached_catalog() {
    let source = r#"{"params":[{"name":"last","group":"Z"},{"name":"CruiseGapLevels","group":"A","min":2,"max":4,"default":4,"options":{"ko":["2","3","4"]}},{"name":"detail","group":"A","detail_parent":"CruiseGapLevels","hidden_brands":["HYUNDAI"]}]}"#;
    let catalog = Catalog::from_data(Value::parse(source).expect("source")).expect("catalog");
    let selected = catalog
        .with_gap_limits(3)
        .expect("gap")
        .for_brand(" Hyundai ")
        .expect("brand");
    assert!(selected
        .groups
        .get("A")
        .encode()
        .expect("encode")
        .contains("\"max\": 3"));
    assert!(catalog
        .groups
        .get("A")
        .encode()
        .expect("encode")
        .contains("\"max\": 4"));
    let Value::Object(groups) = selected.groups else {
        panic!("groups")
    };
    assert_eq!(groups[0].0, vec![u32::from('Z')]);
    assert_eq!(groups[1].0, vec![u32::from('A')]);
}

#[test]
fn settings_reload_uses_integer_mtime_even_when_contents_change() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("settings.json");
    fs::write(&path, r#"{"params":[],"apilot":1}"#).expect("source");
    let stamp = UNIX_EPOCH + Duration::from_secs(100);
    fs::File::open(&path)
        .expect("file")
        .set_times(fs::FileTimes::new().set_modified(stamp))
        .expect("mtime");
    let mut cache = SettingsCache::new(path.clone());
    assert!(cache
        .load(4)
        .expect("cache")
        .data
        .get("apilot")
        .number_eq(1));
    fs::write(&path, r#"{"params":[],"apilot":2}"#).expect("change");
    fs::File::open(&path)
        .expect("file")
        .set_times(fs::FileTimes::new().set_modified(stamp + Duration::from_millis(500)))
        .expect("mtime");
    assert!(cache
        .load(4)
        .expect("same second")
        .data
        .get("apilot")
        .number_eq(1));
    fs::File::open(&path)
        .expect("file")
        .set_times(fs::FileTimes::new().set_modified(stamp + Duration::from_secs(1)))
        .expect("mtime");
    assert!(cache
        .load(4)
        .expect("new second")
        .data
        .get("apilot")
        .number_eq(2));
}

#[test]
fn migration_keeps_newer_state_and_original_file_timestamp() {
    let root = tempfile::tempdir().expect("root");
    let mut config = Config::at(
        root.path(),
        &root.path().join("data"),
        &root.path().join("settings"),
    );
    config.legacy_state = root.path().join("legacy");
    fs::create_dir_all(&config.legacy_state).expect("legacy");
    fs::create_dir_all(&config.state).expect("state");
    fs::write(config.legacy_state.join("git.json"), "old").expect("old");
    fs::write(config.state.join("git.json"), "new").expect("new");
    let source = config.legacy_state.join("web_settings.json");
    fs::write(&source, "web").expect("source");
    let stamp = UNIX_EPOCH + Duration::from_secs(123);
    fs::File::open(&source)
        .expect("file")
        .set_times(fs::FileTimes::new().set_modified(stamp))
        .expect("mtime");
    config.migrate_legacy_state();
    config.migrate_legacy_state();
    assert_eq!(
        fs::read_to_string(config.state.join("git.json")).expect("new state"),
        "new"
    );
    assert_eq!(
        fs::metadata(config.state.join("web_settings.json"))
            .expect("copied")
            .modified()
            .expect("mtime"),
        stamp
    );
}

#[test]
fn native_params_keep_validated_namespace_and_unregistered_atomic_fallback() {
    let root = tempfile::tempdir().expect("root");
    let params =
        openpilot_params::Params::open(&root.path().join("params"), "owned").expect("params");
    let directory = params.directory().to_owned();
    let mut backend = Backend::native(params, root.path().join("state"));
    let setting = Value::parse(r#"{"min":0,"max":60,"default":0}"#).expect("definition");
    backend
        .put("FutureIntSetting", &Value::Float(14.999), Some(&setting))
        .expect("atomic put");
    assert_eq!(
        fs::read(directory.join("FutureIntSetting")).expect("raw bytes"),
        b"15"
    );
    assert!(backend
        .get("FutureIntSetting", &Value::integer(0))
        .number_eq(15));
    assert!(backend.get("../outside", &Value::integer(7)).number_eq(7));
    assert!(backend
        .put("../outside", &Value::integer(1), Some(&setting))
        .is_err());
    assert!(backend.put("Unknown", &Value::integer(1), None).is_err());
    assert!(!root.path().join("outside").exists());
}

#[test]
fn inferred_integer_uses_python_ties_even_and_float32_writes_are_raw() {
    let setting = Value::parse(r#"{"min":0,"max":60,"default":0}"#).expect("definition");
    for (input, expected) in [
        (14.999, 15),
        (0.5, 0),
        (1.5, 2),
        (2.5, 2),
        (-0.5, 0),
        (-1.5, -2),
    ] {
        let (_, output) = coerce_inferred(&Value::Float(input), &setting).expect("coerce");
        assert!(output.number_eq(expected));
    }
    assert!(coerce_inferred(&Value::Null, &setting).is_err());
    let root = tempfile::tempdir().expect("root");
    let params = openpilot_params::Params::open(root.path(), "owned").expect("params");
    let directory = params.directory().to_owned();
    let mut backend = Backend::native(params, root.path().join("state"));
    backend
        .put("UptimeOnroad", &Value::Float(0.1), None)
        .expect("registered float");
    assert_eq!(
        fs::read(directory.join("UptimeOnroad")).expect("raw float"),
        b"0.100000"
    );
}
