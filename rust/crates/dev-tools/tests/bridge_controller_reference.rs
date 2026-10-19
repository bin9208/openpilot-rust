//! Mandatory cloud pairing; missing original/native reports is an error.
#[test]
fn actual_original_and_rust_controllers_agree_on_both_binaries_and_directions() {
    let read = |key| {
        let path = std::env::var_os(key)
            .unwrap_or_else(|| panic!("required cloud comparison report missing: {key}"));
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    };
    openpilot_dev_tools::bridge_check::compare_reports(
        &read("BRIDGE_ORIGINAL_CONTROLLER_REPORT"),
        &read("BRIDGE_NATIVE_CONTROLLER_REPORT"),
    )
    .unwrap();
}
