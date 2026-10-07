#![cfg(feature = "native-skip-miri")]
use openpilot_pandad::{health::Health, supervisor::Log, supervisor_runtime::record};

#[test]
fn health_log_retains_source_keys_and_nonfinite_numbers() {
    let health = Health {
        tx_overflow: 17,
        safety_model: 9,
        interrupt_load: f32::NAN,
        som_reset_triggered: 1,
        ..Health::default()
    };
    let entry = record(Log::SomReset {
        health,
        serial: "test".into(),
    })
    .unwrap();
    let mut fields = openpilot_logging::Fields::new();
    fields.insert("msg".into(), entry.message);
    let text = fields.to_json().unwrap();
    assert!(text.contains("\"tx_buffer_overflow\": 17"));
    assert!(text.contains("\"safety_mode\": 9"));
    assert!(text.contains("\"interrupt_load\": NaN"));
    assert!(text.contains("\"event\": \"panda.som_reset_triggered\""));
    assert!(!text.contains("\"tx_overflow\""));
}

#[test]
fn text_log_uses_python_string_representation() {
    let entry = record(Log::DevelopmentBootloader {
        version: "boot'v\n\u{200b}".into(),
        internal: true,
    })
    .unwrap();
    let openpilot_logging::Value::Text(text) = entry.message else {
        panic!("text expected")
    };
    assert_eq!(text, "Flashed firmware not booting, flashing development bootloader. bootstub_version=\"boot'v\\n\\u200b\", internal_panda=True");
}
