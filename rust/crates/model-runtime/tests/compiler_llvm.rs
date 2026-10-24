#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::compiler_backend_cpu::llvm::{Config, Receipt};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
fn fixture() -> (Config, Value) {
    let library =
        PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required"));
    let data: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-llvm-provider.json"
    ))
    .unwrap();
    let bytes = fs::read(&library).expect("mandatory configured LLVM library is unavailable");
    use sha2::{Digest, Sha256};
    let pin = format!("{:x}", Sha256::digest(bytes));
    let provider = data["providers"].as_array().unwrap().iter().find(|p| p["llvm_library_sha256"] == pin).expect("LLVM library has no independent original capture: configure a captured provider explicitly").clone();
    (
        Config {
            library,
            library_sha256: pin,
            triple: provider["triple"].as_str().unwrap().into(),
            cpu: provider["cpu"].as_str().unwrap().into(),
            features: provider["features"].as_str().unwrap().into(),
            passes: provider["passes"].as_str().unwrap().into(),
            loop_unrolling: true,
            loop_vectorization: true,
            slp_vectorization: true,
            verify_each: true,
        },
        provider,
    )
}
fn run(config: &Config, source: &[u8], dir: &std::path::Path) -> std::process::Output {
    run_paths(
        config,
        source,
        dir,
        &dir.join("receipt"),
        &dir.join("optimized"),
    )
}
fn run_paths(
    config: &Config,
    source: &[u8],
    dir: &std::path::Path,
    receipt: &std::path::Path,
    optimized: &std::path::Path,
) -> std::process::Output {
    fs::write(dir.join("config"), serde_json::to_vec(config).unwrap()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_offline-llvm-compile"))
        .args(["--config"])
        .arg(dir.join("config"))
        .arg("--object")
        .arg(dir.join("object"))
        .arg("--receipt")
        .arg(receipt)
        .arg("--optimized-ir")
        .arg(optimized)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source);
    child.wait_with_output().unwrap()
}
#[test]
fn same_provider_original_objects_and_optimized_ir() {
    let (config, provider) = fixture();
    for case in provider["cases"].as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let output = run(
            &config,
            case["source"].as_str().unwrap().as_bytes(),
            dir.path(),
        );
        assert!(
            output.status.success(),
            "{}: {}",
            case["name"],
            String::from_utf8_lossy(&output.stderr)
        );
        let receipt: Receipt =
            serde_json::from_slice(&fs::read(dir.path().join("receipt")).unwrap()).unwrap();
        assert_eq!(receipt.source_sha256, case["source_sha256"]);
        assert_eq!(receipt.object_sha256, case["object_sha256"]);
        assert_eq!(receipt.optimized_ir_sha256, case["optimized_ir_sha256"]);
    }
}
#[test]
fn invalid_inputs_do_not_publish_any_outputs() {
    let (config, provider) = fixture();
    let source = provider["cases"][0]["source"].as_str().unwrap().as_bytes();
    for kind in ["pin", "parse", "target", "passes", "oversize"] {
        let mut config = config.clone();
        let dir = tempfile::tempdir().unwrap();
        let data = match kind {
            "pin" => {
                config.library_sha256 = "0".repeat(64);
                source.to_vec()
            }
            "parse" => b"not llvm".to_vec(),
            "target" => {
                config.triple = "invalid-no-target".into();
                source.to_vec()
            }
            "passes" => {
                config.passes = "no-such-pass".into();
                source.to_vec()
            }
            _ => vec![b' '; 16 * 1024 * 1024 + 1],
        };
        let output = run(&config, &data, dir.path());
        assert!(!output.status.success(), "{kind}");
        for name in ["object", "receipt", "optimized"] {
            assert!(!dir.path().join(name).exists(), "{kind}: {name}");
        }
    }
}
#[test]
fn existing_and_late_output_failures_preserve_foreign_files() {
    let (config, provider) = fixture();
    let source = provider["cases"][0]["source"].as_str().unwrap().as_bytes();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("receipt"), b"foreign").unwrap();
    assert!(!run(&config, source, dir.path()).status.success());
    assert_eq!(fs::read(dir.path().join("receipt")).unwrap(), b"foreign");
    assert!(!dir.path().join("object").exists());
    assert!(!dir.path().join("optimized").exists());
}

#[test]
fn late_creation_failure_rolls_back_owned_products() {
    use std::os::unix::fs::PermissionsExt;
    let (config, provider) = fixture();
    let source = provider["cases"][0]["source"].as_str().unwrap().as_bytes();
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("blocked");
    fs::create_dir(&blocked).unwrap();
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o500)).unwrap();
    let output = run_paths(
        &config,
        source,
        dir.path(),
        &blocked.join("receipt"),
        &dir.path().join("optimized"),
    );
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Permission denied"));
    for path in [
        dir.path().join("object"),
        dir.path().join("optimized"),
        blocked.join("receipt"),
    ] {
        assert!(!path.exists());
    }
    let output = run_paths(
        &config,
        source,
        dir.path(),
        &dir.path().join("object"),
        &dir.path().join("optimized"),
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("alias"));
}
#[test]
fn parent_contains_crash_timeout_and_validates_receipt() {
    use openpilot_model_runtime::compiler_backend_cpu::llvm::compile_isolated;
    use std::{os::unix::fs::PermissionsExt, time::Duration};
    let (config, provider) = fixture();
    let source = provider["cases"][0]["source"].as_str().unwrap();
    let good = compile_isolated(
        std::path::Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
        &config,
        source,
        Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(
        good.receipt.object_sha256,
        provider["cases"][0]["object_sha256"]
    );
    let temp = tempfile::tempdir().unwrap();
    let worker = temp.path().join("worker");
    for (script, expected) in [
        ("#!/bin/sh\nkill -ABRT $$\n", "LLVM worker"),
        ("#!/bin/sh\nexec sleep 10\n", "timeout"),
    ] {
        fs::write(&worker, script).unwrap();
        fs::set_permissions(&worker, fs::Permissions::from_mode(0o700)).unwrap();
        let error = compile_isolated(&worker, &config, source, Duration::from_millis(40))
            .err()
            .unwrap();
        assert!(error.contains(expected), "{error}");
    }
    // Correctly formed receipt claiming an unrelated object is never trusted.
    let receipt = serde_json::to_string(&good.receipt).unwrap();
    fs::write(&worker,format!("#!/bin/sh\nprintf '\\177ELFfake' > \"$4\"\nprintf x > \"$8\"\ncat > \"$6\" <<'RECEIPT'\n{receipt}\nRECEIPT\n")).unwrap();
    let error = compile_isolated(&worker, &config, source, Duration::from_secs(5))
        .err()
        .unwrap();
    assert!(error.contains("identity mismatch"), "{error}");
}
