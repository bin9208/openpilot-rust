//! Observe the existing live proof through its registered native CLI exactly once.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, os::unix::ffi::OsStrExt, path::Path, process::Command};

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").unwrap();
    }
    text
}

pub fn run(binary: &Path, args: Vec<OsString>, output: &Path) -> Value {
    let binary_before = format!("{:x}", Sha256::digest(fs::read(binary).unwrap()));
    assert!(!output.exists());
    let captured = Command::new(binary).args(&args).output().unwrap();
    let binary_after = format!("{:x}", Sha256::digest(fs::read(binary).unwrap()));
    let receipt = json!({
        "source":std::env::var("EXPECTED_SOURCE").expect("cloud must bind exact source"),
        "binary_sha256_before":binary_before,"binary_sha256_after":binary_after,
        "argv_hex":std::iter::once(binary.as_os_str()).chain(args.iter().map(|a|a.as_os_str()))
            .map(|a|hex(a.as_bytes())).collect::<Vec<_>>(),
        "status":captured.status.code(),"stdout_hex":hex(&captured.stdout),"stderr_hex":hex(&captured.stderr),
        "output_absent_before":true,"output_directory_after":output.is_dir(),
    });
    fs::write(
        output.with_extension("cli.json"),
        format!("{}\n", serde_json::to_string_pretty(&receipt).unwrap()),
    )
    .unwrap();
    assert_eq!(binary_before, binary_after, "native CLI ELF changed");
    assert!(
        captured.status.success(),
        "native CLI failed: {}",
        String::from_utf8_lossy(&captured.stderr)
    );
    assert!(
        captured.stderr.is_empty(),
        "native controller emitted unexpected stderr"
    );
    let report: Value = serde_json::from_slice(&captured.stdout)
        .expect("native CLI must print one complete JSON report");
    let disk: Value =
        serde_json::from_slice(&fs::read(output.join("report.json")).unwrap()).unwrap();
    assert_eq!(
        report, disk,
        "CLI stdout differs from completed disk report"
    );
    report
}
