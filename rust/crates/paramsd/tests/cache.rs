use openpilot_paramsd::cache::{migrate, Store};
use std::collections::BTreeMap;

#[derive(Default)]
struct Memory {
    values: BTreeMap<&'static str, Vec<u8>>,
    removed: Vec<&'static str>,
}
impl Store for Memory {
    fn get(&mut self, key: &'static str) -> Option<Vec<u8>> {
        self.values.get(key).cloned()
    }
    fn put(&mut self, key: &'static str, bytes: Vec<u8>) {
        self.values.insert(key, bytes);
    }
    fn remove(&mut self, key: &'static str) {
        self.values.remove(key);
        self.removed.push(key);
    }
}
#[test]
fn legacy_nonfinite_and_utf16_migration_owns_result() -> Result<(), Box<dyn std::error::Error>> {
    let text = "{\"steerRatio\":17,\"stiffnessFactor\":NaN,\"angleOffsetAverageDeg\":2}";
    let encodings = [
        text.as_bytes().to_vec(),
        text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
    ];
    for bytes in encodings {
        let mut store = Memory::default();
        store.values.insert("LiveParameters", bytes);
        let mut logs = Vec::new();
        migrate(&mut store, 123, &mut logs);
        let bytes = store
            .values
            .remove("LiveParametersV2")
            .ok_or("migration failed")?;
        drop(store);
        let message =
            capnp::serialize::read_message(bytes.as_slice(), capnp::message::ReaderOptions::new())?;
        let event = message.get_root::<openpilot_cereal::log_capnp::event::Reader<'_>>()?;
        assert!(!event.get_valid());
        let openpilot_cereal::log_capnp::event::LiveParameters(value) = event.which()? else {
            return Err("wrong event".into());
        };
        let value = value?;
        assert!(value.get_valid());
        assert_eq!(value.get_steer_ratio(), 17.);
        assert!(value.get_stiffness_factor().is_nan());
        assert!(logs.is_empty());
    }
    Ok(())
}
#[test]
fn invalid_json_is_retained_but_missing_fields_remove_old_value() {
    for (bytes, removed) in [(b"{".as_slice(), false), (b"{}".as_slice(), true)] {
        let mut store = Memory::default();
        store.values.insert("LiveParameters", bytes.to_vec());
        let mut logs = Vec::new();
        migrate(&mut store, 123, &mut logs);
        assert_eq!(store.removed.contains(&"LiveParameters"), removed);
        assert!(!store.values.contains_key("LiveParametersV2"));
        assert_eq!(logs.len(), 1);
    }
}
