#![cfg(feature = "image-array-native")]
use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_emoji_check::check,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
struct Peer {
    mode: String,
    calls: usize,
    fixtures: PathBuf,
}
impl Runner for Peer {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        let argv = command.effective_argv();
        assert_eq!(argv.len(), 4);
        assert_eq!(argv[0], "candidate");
        assert_eq!(command.timeout, Duration::MAX);
        assert!(command.inherit_output && command.inherit_stdin);
        let target = Path::new(&argv[3]);
        let failed = self.mode == "child" || (self.mode == "late-child" && self.calls == 3);
        if !failed && self.mode != "missing" {
            if self.mode == "rewrite-source" {
                let image = self.fixtures.join(format!("pixel-{}.png", self.calls - 1));
                fs::copy(
                    &image,
                    target.with_file_name(format!("emoji-{}-source.png", self.calls - 1)),
                )?;
                fs::copy(image, target)?;
            } else if self.mode == "invalid" {
                fs::write(target, b"invalid")?;
            } else if ["pixel", "rgb", "gray", "palette", "broadcast"].contains(&self.mode.as_str())
            {
                fs::copy(
                    self.fixtures
                        .join(format!("{}-{}.png", self.mode, self.calls - 1)),
                    target,
                )?;
            } else {
                fs::copy(
                    target.with_file_name(format!("emoji-{}-source.png", self.calls - 1)),
                    target,
                )?;
            }
        }
        Ok(Captured {
            code: if failed { 7 } else { 0 },
            stdout: vec![],
            stderr: vec![],
        })
    }
}
#[test]
fn original_seven_rasters_and_all_failure_artifact_boundaries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-emoji");
    let original: Value =
        serde_json::from_str(include_str!("fixtures/ui-emoji/original.json")).unwrap();
    for case in original["cases"].as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let mode = case["mode"].as_str().unwrap();
        if mode == "report-dir" {
            fs::create_dir_all(out.join("emoji-pixels.json")).unwrap();
        }
        let mut peer = Peer {
            mode: mode.into(),
            calls: 0,
            fixtures: fixtures.clone(),
        };
        let mut printed = vec![];
        let result = check(&root, Path::new("candidate"), &out, &mut peer, &mut printed);
        assert_eq!(
            result.is_ok(),
            case["error"].is_null(),
            "{mode}: {result:?}"
        );
        assert_eq!(
            peer.calls,
            case["calls"].as_u64().unwrap() as usize,
            "{mode}"
        );
        let mut actual = fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_file())
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        actual.sort();
        let mut expected = case["files"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(actual, expected, "{mode}");
        for file in &actual {
            let bytes = fs::read(out.join(file)).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(bytes)),
                case["files"][file].as_str().unwrap(),
                "{mode}/{file}"
            );
        }
        if let Some(report) = case.get("report") {
            let text = fs::read_to_string(out.join("emoji-pixels.json")).unwrap();
            assert_eq!(
                &serde_json::from_str::<Value>(&text).unwrap(),
                report,
                "{mode}"
            );
        }
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            case["stdout"].as_str().unwrap(),
            "{mode}"
        );
    }
}
#[test]
fn pinned_original_and_font_changes_fail_before_invoking_candidate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let original: Value =
        serde_json::from_str(include_str!("fixtures/ui-emoji/original.json")).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    fs::copy(root.join("uv.lock"), tmp.path().join("uv.lock")).unwrap();
    for path in original["sources"].as_object().unwrap().keys() {
        let dest = tmp.path().join(path);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        if let Ok(bytes) = fs::read(root.join(path)) {
            fs::write(dest, bytes).unwrap();
        }
    }
    for path in original["sources"].as_object().unwrap().keys() {
        let dest = tmp.path().join(path);
        let before = fs::read(&dest).ok();
        fs::write(&dest, b"changed").unwrap();
        let mut peer = Peer {
            mode: "exact".into(),
            calls: 0,
            fixtures: PathBuf::new(),
        };
        let result = check(
            tmp.path(),
            Path::new("candidate"),
            &tmp.path().join("output"),
            &mut peer,
            &mut vec![],
        );
        assert!(result.is_err(), "{path}");
        assert_eq!(peer.calls, 0);
        if let Some(bytes) = before {
            fs::write(dest, bytes).unwrap();
        } else {
            fs::remove_file(dest).unwrap();
        }
    }
}
struct BroadPeer {
    mode: String,
    calls: usize,
    directory: PathBuf,
}
impl Runner for BroadPeer {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        let argv = command.effective_argv();
        let target = Path::new(&argv[3]);
        let source = self
            .directory
            .join(&self.mode)
            .join(format!("emoji-{}-native.png", self.calls));
        self.calls += 1;
        if source.exists() {
            fs::copy(source, target)?;
        }
        Ok(Captured {
            code: if self.mode.starts_with("failure-") {
                7
            } else {
                0
            },
            stdout: vec![],
            stderr: vec![],
        })
    }
}
#[test]
fn original_broad_decoders_broadcasting_and_hidden_rgb_remain_covered() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/ui-emoji/original-cases.json")).unwrap();
    for case in cases["cases"].as_array().unwrap() {
        let mode = case["case"].as_str().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("output");
        let mut peer = BroadPeer {
            mode: mode.into(),
            calls: 0,
            directory: Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-emoji/broad"),
        };
        let mut printed = vec![];
        let result = check(&root, Path::new("candidate"), &out, &mut peer, &mut printed);
        assert_eq!(result.is_ok(), case["exit"] == 0, "{mode}: {result:?}");
        assert_eq!(peer.calls as u64, case["calls"].as_u64().unwrap(), "{mode}");
        let mut artifacts = fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        artifacts.sort();
        assert_eq!(serde_json::json!(artifacts), case["artifacts"], "{mode}");
        if !case["result"].is_null() {
            let actual: Value =
                serde_json::from_slice(&fs::read(out.join("emoji-pixels.json")).unwrap()).unwrap();
            assert_eq!(actual, case["result"], "{mode}");
        }
        let expected = case["stdout"].as_str().unwrap().replace("peer-out\n", "");
        assert_eq!(String::from_utf8(printed).unwrap(), expected, "{mode}");
    }
}
#[test]
fn checker_cli_requires_original_arguments_and_preserves_help() {
    let binary = env!("CARGO_BIN_EXE_check-ui-emoji");
    let result = std::process::Command::new(binary)
        .arg("--help")
        .output()
        .unwrap();
    assert!(result.status.success());
    for args in [
        vec![],
        vec!["--binary", "candidate"],
        vec!["--output", "out"],
        vec!["--unknown", "x"],
    ] {
        let result = std::process::Command::new(binary)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
    }
}
#[test]
fn real_cli_child_streams_and_stdin_remain_inherited() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("peer");
    let out = dir.path().join("out");
    fs::write(&child,b"#!/bin/bash\nread -r token\nprintf 'stdout:%s\\n' \"$token\"\nprintf 'stderr:%s\\n' \"$token\" >&2\nsource_image=${3%-native.png}-source.png\n/bin/cp \"$source_image\" \"$3\"\n").unwrap();
    fs::set_permissions(&child, fs::Permissions::from_mode(0o755)).unwrap();
    let mut process = Command::new(env!("CARGO_BIN_EXE_check-ui-emoji"))
        .env("OPENPILOT_RUST_ROOT", root)
        .env("PATH", "/nonexistent")
        .args([
            "--binary",
            child.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(b"0\n1\n2\n3\n4\n5\n6\n")
        .unwrap();
    let result = process.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8(result.stdout).unwrap();
    let stderr = String::from_utf8(result.stderr).unwrap();
    for index in 0..7 {
        assert!(stdout.contains(&format!("stdout:{index}\n")));
        assert!(stderr.contains(&format!("stderr:{index}\n")));
    }
    assert!(stdout.ends_with("skin tone\n"));
}
#[test]
fn independently_captured_dependency_versions_cannot_silently_drift() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let original: Value =
        serde_json::from_str(include_str!("fixtures/ui-emoji/original.json")).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    for path in original["sources"].as_object().unwrap().keys() {
        let dest = tmp.path().join(path);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        if let Ok(bytes) = fs::read(root.join(path)) {
            fs::write(dest, bytes).unwrap();
        }
    }
    let lock = fs::read_to_string(root.join("uv.lock")).unwrap();
    for (package, version) in [("numpy", "2.5.3"), ("pillow", "12.3.0")] {
        let needle = format!("\nname = \"{package}\"\nversion = \"{version}\"");
        assert_eq!(lock.matches(&needle).count(), 1);
        let changed = lock.replace(
            &needle,
            &format!("\nname = \"{package}\"\nversion = \"99.0.0\""),
        );
        fs::write(tmp.path().join("uv.lock"), changed).unwrap();
        let mut peer = Peer {
            mode: "exact".into(),
            calls: 0,
            fixtures: PathBuf::new(),
        };
        let result = check(
            tmp.path(),
            Path::new("candidate"),
            &tmp.path().join(package),
            &mut peer,
            &mut vec![],
        );
        assert!(result.is_err(), "{package}");
        assert_eq!(peer.calls, 0, "{package}");
    }
}
