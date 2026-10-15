use super::*;
#[test]
fn source_platform_dimensions_and_backlight_files() {
    let rows: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/startup-recovery-platform-final.json"
    ))
    .unwrap();
    for row in rows.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let panel = root.join("sys/class/backlight/panel0-backlight");
        fs::create_dir_all(&panel).unwrap();
        if let Some(model) = row["model"].as_str() {
            let path = root.join("sys/firmware/devicetree/base/model");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, model).unwrap();
        }
        if let Some(value) = row["maximum"].as_str() {
            fs::write(panel.join("max_brightness"), value).unwrap();
        }
        assert_eq!(serde_json::json!(device_size(root)), row["size"]);
        wake_display(root);
        assert_eq!(
            fs::read_to_string(panel.join("bl_power")).unwrap(),
            row["power"]
        );
        assert_eq!(
            serde_json::json!(fs::read_to_string(panel.join("brightness")).ok()),
            row["brightness"]
        );
    }
}
