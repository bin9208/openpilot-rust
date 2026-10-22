use std::process::Command;

#[test]
fn cli_reports_available_space_and_rejects_impossible_growth() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_disk-space-guard"))
        .arg("--path")
        .arg(temp.path().join("new/cache"))
        .args(["--growth-mib", "1000000000"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["growth_bytes"], 1_000_000_000u64 * 1024 * 1024);
    assert_eq!(report["required_bytes"], 1_000_000_000u64 * 1024 * 1024);
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("insufficient disk for estimated operation growth"));
}

#[test]
fn cli_rejects_negative_overflowing_and_duplicate_options() {
    for args in [
        vec!["--path", ".", "--growth-mib", "-1"],
        vec!["--path", ".", "--growth-mib", "18446744073709551615"],
        vec!["--path", ".", "--path", ".", "--growth-mib", "0"],
        vec!["--path", "."],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_disk-space-guard"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn cli_preserves_nonexistent_non_utf8_paths_and_zero_growth() {
    use std::os::unix::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp
        .path()
        .join(std::ffi::OsString::from_vec(b"not-utf8-\xff/new".to_vec()));
    let output = Command::new(env!("CARGO_BIN_EXE_disk-space-guard"))
        .arg("--path")
        .arg(&path)
        .args(["--growth-mib", "0"])
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["growth_bytes"], 0);
    assert_eq!(
        report["required_bytes"],
        openpilot_disk_space::RESERVE_BYTES
    );
    assert_eq!(
        output.status.success(),
        report["free_bytes"].as_u64().unwrap() >= openpilot_disk_space::RESERVE_BYTES
    );
    assert!(!path.exists());
}

#[test]
fn largest_representable_growth_is_reported_without_the_retired_reserve() {
    let output = Command::new(env!("CARGO_BIN_EXE_disk-space-guard"))
        .args(["--path", ".", "--growth-mib", "17592186044415"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["required_bytes"],
        17_592_186_044_415u64 * 1024 * 1024
    );
    assert_eq!(report["required_bytes"], report["growth_bytes"]);
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("insufficient disk for estimated operation growth"));
}
