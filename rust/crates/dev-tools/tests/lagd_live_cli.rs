//! Actual native entrypoints, immutable original CLI captures and interruption.
#![cfg(feature = "lagd-tools-native")]
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
#[path = "support/lagd_cli_original.rs"]
mod expected;
fn binary(name: &str) -> &'static str {
    match name {
        "numeric" => env!("CARGO_BIN_EXE_check-lagd-numeric"),
        "loop" => env!("CARGO_BIN_EXE_check-lagd-loop"),
        "cache" => env!("CARGO_BIN_EXE_check-lagd-cache"),
        "packet" => env!("CARGO_BIN_EXE_check-lagd-packet"),
        "daemon" => env!("CARGO_BIN_EXE_check-lagd-daemon"),
        _ => env!("CARGO_BIN_EXE_check-lagd-params-io"),
    }
}
fn files(root: &Path, at: &Path) -> serde_json::Map<String, Value> {
    let mut rows = serde_json::Map::new();
    for entry in fs::read_dir(at).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rows.extend(files(root, &path));
        } else {
            rows.insert(
                path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                Value::String(format!("{:x}", Sha256::digest(fs::read(path).unwrap()))),
            );
        }
    }
    rows
}
#[test]
fn all_42_original_cli_admissions_and_actual_process_file_effects() {
    let original = expected::cli();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for row in original["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let args = row["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                v.as_str()
                    .unwrap()
                    .replace("@CASE@", temporary.path().to_str().unwrap())
            })
            .collect::<Vec<_>>();
        let output = Command::new(binary(row["tool"].as_str().unwrap()))
            .args(&args)
            .env("OPENPILOT_RUST_ROOT", &root)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            row["returncode"].as_i64().map(|v| v as i32),
            "{} {}: {}",
            row["tool"],
            row["case"],
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            row["stdout"].as_str().unwrap()
        );
        assert_eq!(
            output.stderr.is_empty(),
            row["stderr"].as_str().unwrap().is_empty()
        );
        let want = row["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, v)| (key.clone(), v["sha256"].clone()))
            .collect::<serde_json::Map<_, _>>();
        assert_eq!(
            files(temporary.path(), temporary.path()),
            want,
            "{} {}",
            row["tool"],
            row["case"]
        );
    }
}
struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn gone_or_zombie(pid: u32) -> bool {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(text) => text
            .rsplit_once(')')
            .is_some_and(|(_, tail)| tail.trim_start().starts_with('Z')),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}
#[test]
fn original_actual_sigint_during_first_peer_readline_cleans_children_and_namespace() {
    let raw = include_bytes!("fixtures/lagd-tools/cli-interrupt-original.json");
    let original: Value = serde_json::from_slice(raw).unwrap();
    assert_eq!(original["returncode"], -2);
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "016778beb98c3775a0a4e9d7a1712fa3308d9b259c3c23b065cd5fdc278cb9e4"
    );
    assert!(fs::read("/bin/cat").unwrap().starts_with(b"\x7fELF"));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    let mut owner = Owned(
        Command::new(binary("daemon"))
            .arg("/bin/true")
            .arg("/bin/cat")
            .arg(&output)
            .env("OPENPILOT_RUST_ROOT", root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let end = Instant::now() + Duration::from_secs(5);
    let mut peer = None;
    let mut prefix = None;
    while Instant::now() < end {
        let children = fs::read_to_string(format!(
            "/proc/{}/task/{}/children",
            owner.0.id(),
            owner.0.id()
        ))
        .unwrap_or_default();
        for id in children
            .split_whitespace()
            .filter_map(|v| v.parse::<u32>().ok())
        {
            if let Ok(env) = fs::read(format!("/proc/{id}/environ")) {
                if let Some(value) = env
                    .split(|v| *v == 0)
                    .find_map(|v| v.strip_prefix(b"OPENPILOT_PREFIX="))
                {
                    prefix = Some(String::from_utf8(value.to_vec()).unwrap());
                    peer = Some(id);
                }
            }
        }
        if peer.is_some() && output.join("learning/peer.log").exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let peer = peer.expect("native cat peer was never spawned");
    let prefix = prefix.unwrap();
    assert!(prefix.starts_with("lagd_"));
    rustix::process::kill_process(
        rustix::process::Pid::from_child(&owner.0),
        rustix::process::Signal::INT,
    )
    .unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = owner.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < end,
            "checker ignored SIGINT while blocked in peer readiness"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(
        status.signal(),
        Some(2),
        "native must preserve original SIGINT termination after its cleanup"
    );
    let end = Instant::now() + Duration::from_secs(2);
    while !gone_or_zombie(peer) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(gone_or_zombie(peer));
    assert!(!Path::new(&format!("/dev/shm/msgq_{prefix}")).exists());
    let actual = files(temp.path(), temp.path());
    let expected = original["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| {
            use base64::Engine;
            let raw = base64::engine::general_purpose::STANDARD
                .decode(value.as_str().unwrap())
                .unwrap();
            (
                key.clone(),
                Value::String(format!("{:x}", Sha256::digest(raw))),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    assert_eq!(actual, expected);
}
