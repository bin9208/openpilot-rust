#![cfg(all(feature = "native-skip-miri", unix))]
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

const SOURCE: &[u8] = b"define float @f(float %x, float %y, i1 %c) {\n %s = select i1 %c, float 0.0, float %y\n %r = fadd nsz arcp contract afn float %s, %x\n ret float %r\n}\n";
fn run(path: &Path, source: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_offline-llvm-guarded-add"))
        .arg("--receipt")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = source.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    output
}
#[test]
fn cli_emits_rewrite_and_hash_receipt_and_preserves_unchanged_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("receipt.json");
    let result = run(&path, SOURCE);
    assert!(result.status.success(), "{:?}", result);
    let expected = b"define float @f(float %x, float %y, i1 %c) {\n %s = select i1 %c, float 0.0, float %y\n %diagnostic_guarded_sum_0 = fadd arcp contract afn float %x, %y\n %r = select i1 %c, float %x, float %diagnostic_guarded_sum_0\n ret float %r\n}\n";
    assert_eq!(result.stdout, expected);
    let receipt: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(receipt["rewrite_count"], 1);
    assert_eq!(
        receipt["original_source_sha256"],
        format!("{:x}", Sha256::digest(SOURCE))
    );
    assert_eq!(
        receipt["output_source_sha256"],
        format!("{:x}", Sha256::digest(expected))
    );
    assert_eq!(
        receipt["rewrites"][0]["retained_flags"],
        serde_json::json!(["arcp", "contract", "afn"])
    );
    let unchanged = b"; opaque input\r\n; byte preservation\r\n";
    let result = run(&dir.path().join("unchanged.json"), unchanged);
    assert!(result.status.success());
    assert_eq!(result.stdout, unchanged);
}
#[test]
fn cli_invalid_utf8_oversize_collision_and_receipt_failure_have_no_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let oversize = vec![b' '; 16 * 1024 * 1024 + 1];
    let collision = String::from_utf8(SOURCE.to_vec()).unwrap().replace(
        " %s =",
        " %diagnostic_guarded_sum_0 = fadd float %x, %y\n %s =",
    );
    for input in [&b"\xff"[..], oversize.as_slice(), collision.as_bytes()] {
        let path = dir.path().join("failed.json");
        let result = run(&path, input);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!path.exists());
    }
    let path = dir.path().join("occupied.json");
    fs::write(&path, b"foreign").unwrap();
    let result = run(&path, SOURCE);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(fs::read(&path).unwrap(), b"foreign");
    let missing = dir.path().join("absent/receipt.json");
    assert!(!run(&missing, SOURCE).status.success());
    assert!(!missing.exists());
}
#[test]
fn cli_requires_explicit_receipt_and_rejects_extra_arguments() {
    for args in [
        vec![],
        vec!["--receipt"],
        vec!["--other", "x"],
        vec!["--receipt", "x", "extra"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_offline-llvm-guarded-add"))
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}
#[test]
fn cli_stdout_failure_removes_its_receipt_and_symlink_collision_survives() {
    let dir = tempfile::tempdir().unwrap();
    let receipt = dir.path().join("receipt.json");
    let mut child = Command::new(env!("CARGO_BIN_EXE_offline-llvm-guarded-add"))
        .arg("--receipt")
        .arg(&receipt)
        .stdin(Stdio::piped())
        .stdout(
            fs::OpenOptions::new()
                .write(true)
                .open("/dev/full")
                .unwrap(),
        )
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(SOURCE).unwrap();
    assert!(!child.wait_with_output().unwrap().status.success());
    assert!(!receipt.exists());
    let target = dir.path().join("target");
    fs::write(&target, b"foreign").unwrap();
    std::os::unix::fs::symlink(&target, &receipt).unwrap();
    let result = run(&receipt, SOURCE);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(fs::symlink_metadata(&receipt)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read(target).unwrap(), b"foreign");
}
