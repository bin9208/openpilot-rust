//! Mandatory live original/native report pair; no synthesized original fallback.
use std::{collections::BTreeMap, fs, path::PathBuf};

fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("required live BlueZ policy evidence missing: {name}"))
}

#[test]
fn original_python_312_and_native_controller_keep_full_corpus_and_receipt() {
    let original = required("BLUEZ_POLICY_ORIGINAL_OUTPUT");
    let native = required("BLUEZ_POLICY_NATIVE_OUTPUT");
    for name in [
        "cases.jsonl",
        "source.json",
        "native.jsonl",
        "native.stderr",
    ] {
        assert_eq!(
            fs::read(original.join(name)).unwrap(),
            fs::read(native.join(name)).unwrap(),
            "original/native file differs: {name}"
        );
    }
    let original_receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(original.join("result.json")).unwrap()).unwrap();
    let native_receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(native.join("result.json")).unwrap()).unwrap();
    assert_eq!(original_receipt, native_receipt);
    assert_eq!(original_receipt["cases"], 6363);
    assert_eq!(original_receipt["digit_points"], 808);
    assert_eq!(original_receipt["unicode_version"], "15.0.0");
    let stdout: serde_json::Value =
        serde_json::from_slice(&fs::read(required("BLUEZ_POLICY_ORIGINAL_STDOUT")).unwrap())
            .unwrap();
    assert_eq!(stdout, original_receipt);
    let native_stdout: serde_json::Value =
        serde_json::from_slice(&fs::read(required("BLUEZ_POLICY_NATIVE_STDOUT")).unwrap()).unwrap();
    assert_eq!(native_stdout, native_receipt);
    let _original_diagnostics = fs::read(required("BLUEZ_POLICY_ORIGINAL_STDERR")).unwrap();
    let _native_diagnostics = fs::read(required("BLUEZ_POLICY_NATIVE_STDERR")).unwrap();
    let digits: serde_json::Value =
        serde_json::from_slice(&fs::read(required("BLUEZ_POLICY_ORIGINAL_DIGITS")).unwrap())
            .unwrap();
    let native_digits: serde_json::Value =
        serde_json::from_slice(&fs::read(native.join("digits.stdout")).unwrap()).unwrap();
    assert_eq!(digits, native_digits);
    assert_eq!(digits.as_array().unwrap().len(), 808);
    assert_eq!(
        fs::read(required("BLUEZ_POLICY_ORIGINAL_DIGITS_STDERR")).unwrap(),
        fs::read(native.join("digits.stderr")).unwrap(),
        "original/native digit probe stderr differs"
    );
}

#[test]
fn same_elf_absence_lane_keeps_all_outputs_and_exact_native_execs() {
    let proof = required("BLUEZ_POLICY_ABSENCE_PROOF");
    let historical = proof.join("root/rust/tools/check_bluetooth_bluez_policy.py");
    assert!(matches!(fs::symlink_metadata(historical), Err(error)
        if error.kind() == std::io::ErrorKind::NotFound));
    let native = required("BLUEZ_POLICY_NATIVE_OUTPUT");
    for name in [
        "cases.jsonl",
        "source.json",
        "native.jsonl",
        "native.stderr",
        "digits.stdout",
        "digits.stderr",
        "result.json",
    ] {
        assert_eq!(
            fs::read(native.join(name)).unwrap(),
            fs::read(proof.join("output").join(name)).unwrap(),
            "absent output differs: {name}"
        );
    }
    for (name, key) in [
        ("native.stdout", "BLUEZ_POLICY_NATIVE_STDOUT"),
        ("native.stderr", "BLUEZ_POLICY_NATIVE_STDERR"),
    ] {
        assert_eq!(
            fs::read(proof.join(name)).unwrap(),
            fs::read(required(key)).unwrap()
        );
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let seals = fs::read_to_string(proof.parent().unwrap().join("artifacts.sha256"))
        .unwrap()
        .lines()
        .map(|line| {
            let (hash, path) = line.split_once("  ").unwrap();
            let path = PathBuf::from(path);
            let path = fs::canonicalize(if path.is_absolute() {
                path
            } else {
                root.join(path)
            })
            .unwrap();
            (path, hash.to_owned())
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(seals.len(), 4);
    for (path, hash) in &seals {
        assert_eq!(&openpilot_dev_tools::seal::digest(path).unwrap(), hash);
    }
    let checker = fs::read_to_string(proof.join("checker.path"))
        .unwrap()
        .trim()
        .to_owned();
    let candidate = fs::read_to_string(proof.join("candidate.path"))
        .unwrap()
        .trim()
        .to_owned();
    for path in [&checker, &candidate] {
        assert!(seals.contains_key(&fs::canonicalize(path).unwrap()));
    }
    let output = proof.join("output").to_str().unwrap().to_owned();
    let expected = BTreeMap::from([
        (
            (
                checker.clone(),
                vec![
                    checker,
                    "--binary".into(),
                    candidate.clone(),
                    "--output".into(),
                    output,
                ],
            ),
            1,
        ),
        ((candidate.clone(), vec![candidate.clone()]), 1),
        ((candidate.clone(), vec![candidate, "--digits".into()]), 1),
    ]);
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let invocation = regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), "#).unwrap();
    let trace = fs::read_to_string(proof.join("native.exec")).unwrap();
    assert_eq!(
        opening.find_iter(&trace).count(),
        3,
        "unexpected exec attempt"
    );
    let mut actual = BTreeMap::new();
    for capture in invocation.captures_iter(&trace) {
        let executable: String = serde_json::from_str(&capture[1]).unwrap();
        let argv: Vec<String> = serde_json::from_str(&capture[2]).unwrap();
        *actual.entry((executable, argv)).or_insert(0) += 1;
    }
    assert_eq!(actual, expected, "opaque or unexpected executable/argv");
}
