#[test]
fn startup_enables_joystick_before_certificate_failure_without_python() {
    let root = tempfile::tempdir().unwrap();
    let params = root.path().join("params");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_openpilot-webjoystick"))
        .args(["--root"])
        .arg(root.path())
        .args(["--openssl", "/nonexistent-native-openssl"])
        .env("PARAMS_ROOT", &params)
        .env_remove("OPENPILOT_PREFIX")
        .env("CEREAL_FAKE", "1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        std::fs::read(params.join("d/JoystickDebugMode")).unwrap(),
        b"1"
    );
}
