use std::process::Command;

#[test]
fn cli_reports_available_space_and_rejects_impossible_growth() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_check-disk-space"))
        .arg("--path")
        .arg(temp.path().join("new/cache"))
        .args(["--growth-mib", "1000000000"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["growth_bytes"], 1_000_000_000u64 * 1024 * 1024);
    assert_eq!(
        report["required_bytes"],
        1_000_000_000u64 * 1024 * 1024 + 25 * 1024 * 1024 * 1024
    );
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("recover at least 35 GiB"));
}

#[test]
fn cli_rejects_negative_overflowing_and_duplicate_options() {
    for args in [
        vec!["--path", ".", "--growth-mib", "-1"],
        vec!["--path", ".", "--growth-mib", "18446744073709551615"],
        vec!["--path", ".", "--path", ".", "--growth-mib", "0"],
        vec!["--path", "."],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_check-disk-space"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}
