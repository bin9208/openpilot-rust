use openpilot_dev_tools::bluetooth_digits_generate::generate;
use std::process::Command;

#[test]
fn pinned_unicode_reproduces_exhaustive_original_digit_generation() {
    // Captured independently by the unchanged original, iterating all 0x110000
    // code points with CPython's Unicode 15 database, before retirement.
    assert_eq!(
        generate().unwrap(),
        include_str!("fixtures/bluetooth-digits/original.rs")
    );
}

#[test]
fn cli_generates_without_python_and_preserves_ignored_arguments() {
    let output = Command::new(env!("CARGO_BIN_EXE_generate-bluetooth-digits"))
        .args(["--unused", "argument"])
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        include_bytes!("fixtures/bluetooth-digits/original.rs")
    );
}
