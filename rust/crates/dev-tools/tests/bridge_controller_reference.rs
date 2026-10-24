//! Mandatory post-retirement cloud traffic compared with sealed actual original output.
fn current_report() -> serde_json::Value {
    let path = std::env::var_os("BRIDGE_NATIVE_CONTROLLER_REPORT")
        .expect("required current bridge controller report missing");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
#[test]
fn fresh_native_controller_matches_sealed_original_for_both_binaries_and_directions() {
    let historical = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rust/tools/check_bridge.py");
    assert!(matches!(std::fs::symlink_metadata(historical), Err(error)
        if error.kind() == std::io::ErrorKind::NotFound));
    openpilot_dev_tools::bridge_check::compare_with_observation(&current_report()).unwrap();
}

#[test]
fn same_elf_absence_run_preserves_original_report_and_only_native_execs() {
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use std::{collections::BTreeMap, fs, path::PathBuf};
    let proof = PathBuf::from(
        std::env::var_os("BRIDGE_ABSENCE_PROOF").expect("mandatory bridge absence proof"),
    );
    let report: Value =
        serde_json::from_slice(&fs::read(proof.join("absent/result.json")).unwrap()).unwrap();
    openpilot_dev_tools::bridge_check::compare_with_observation(&report).unwrap();
    openpilot_dev_tools::bridge_check::compare_reports(&current_report(), &report).unwrap();
    let artifacts: BTreeMap<String, String> = fs::read_to_string(proof.join("artifacts.sha256"))
        .unwrap()
        .lines()
        .map(|line| {
            let (digest, path) = line.split_once("  ").expect("artifact SHA256 record");
            (path.into(), digest.into())
        })
        .collect();
    assert_eq!(artifacts.len(), 5, "checker, two bridges, unshare and ip");
    for (path, expected) in &artifacts {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            *expected
        );
    }
    let current_path = PathBuf::from(std::env::var_os("BRIDGE_NATIVE_CONTROLLER_REPORT").unwrap());
    let present: Value = serde_json::from_slice(
        &fs::read(current_path.parent().unwrap().join("provenance.json")).unwrap(),
    )
    .unwrap();
    let absent: Value =
        serde_json::from_slice(&fs::read(proof.join("absent/provenance.json")).unwrap()).unwrap();
    for (role, key) in [
        ("checker", "controller_sha256"),
        ("original", "original_sha256"),
        ("native", "native_sha256"),
    ] {
        let path = fs::read_to_string(proof.join(format!("{role}.path"))).unwrap();
        let expected = &artifacts[path.trim()];
        assert_eq!(
            present[key].as_str(),
            Some(expected.as_str()),
            "present {role}"
        );
        assert_eq!(
            absent[key].as_str(),
            Some(expected.as_str()),
            "absent {role}"
        );
    }
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let executable = regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*")"#).unwrap();
    let mut attempts = BTreeMap::<String, usize>::new();
    for line in fs::read_to_string(proof.join("native.exec"))
        .unwrap()
        .lines()
    {
        if !opening.is_match(line) {
            continue;
        }
        let capture = executable
            .captures(line)
            .unwrap_or_else(|| panic!("opaque/unaccounted execve or execveat: {line}"));
        let path: String = serde_json::from_str(&capture[1]).unwrap();
        assert!(
            artifacts.contains_key(&path),
            "unsealed executable attempt: {line}"
        );
        *attempts.entry(path).or_default() += 1;
    }
    assert_eq!(attempts.len(), 5, "all sealed native roles must execute");
    // One checker plus four private cases. Each case executes unshare, the
    // checker worker, ip and exactly one of the original/native bridge binaries.
    assert_eq!(attempts.values().sum::<usize>(), 17);
    for (role, count) in [
        ("checker", 5),
        ("original", 2),
        ("native", 2),
        ("unshare", 4),
        ("ip", 4),
    ] {
        let path = fs::read_to_string(proof.join(format!("{role}.path"))).unwrap();
        assert_eq!(attempts.get(path.trim()), Some(&count), "{role}");
    }
    let absent_source = proof.join("native-root/rust/tools/check_bridge.py");
    assert!(
        matches!(fs::symlink_metadata(absent_source), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
    );
    fs::write(
        proof.join("comparison.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "passed":true,"same_elf":true,"scenarios":4,"packets_per_direction":160,
            "python_exec_attempts":0,"exec_attempts":attempts,"artifacts":artifacts
        }))
        .unwrap(),
    )
    .unwrap();
}
