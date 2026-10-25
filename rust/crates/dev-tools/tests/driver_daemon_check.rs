use openpilot_dev_tools::driver_daemon_check::{parse, run, Options, OriginalCapture};
use std::{env, ffi::OsString, fs, os::unix::ffi::OsStringExt, path::PathBuf};

fn cli_args() -> Vec<OsString> {
    [
        "--binary",
        "/private/driver",
        "--catalog",
        "/private/catalog",
        "--models",
        "/private/models",
        "--output",
        "/private/output",
        "--collector",
        "/private/collector",
        "--native-peer",
        "/private/peer",
        "--native-original-dir",
        "/private/original",
        "--native-original-manifest-sha256",
        "a",
        "--native-source-commit",
        "b",
        "--native-root",
        "/private/root",
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

#[test]
fn driver_cli_retains_argparse_help_abbreviation_repeat_and_raw_path() {
    assert!(parse(&[OsString::from("-hhh")]).unwrap().is_none());
    let mut args = cli_args();
    args[0] = "--bin".into();
    args[6] = "--o".into();
    args.extend([
        OsString::from("--binary"),
        OsString::from("/private/final-driver"),
    ]);
    let selected = parse(&args).unwrap().unwrap();
    assert_eq!(selected.binary, PathBuf::from("/private/final-driver"));
    let raw = OsString::from_vec(b"/private/output-\xff".to_vec());
    args[7] = raw.clone();
    let selected = parse(&args).unwrap().unwrap();
    assert_eq!(selected.output.as_os_str(), raw.as_os_str());
}

#[test]
fn driver_cli_rejects_missing_ambiguous_and_unknown_arguments() {
    assert!(parse(&[]).is_err());
    assert!(parse(&["--c".into(), "value".into()]).is_err());
    let mut args = cli_args();
    args.extend([OsString::from("--unknown"), OsString::from("value")]);
    assert!(parse(&args).is_err());
}

#[test]
fn driver_cli_requires_paired_current_receipt_and_explicit_retirement_policy() {
    use openpilot_dev_tools::driver_daemon_check::SourcePolicy;
    let original = parse(&cli_args()).unwrap().unwrap();
    assert_eq!(original.source_policy, SourcePolicy::Present);
    assert!(original.current_catalog_receipt.is_none());
    for option in [
        "--native-current-catalog-receipt",
        "--native-current-catalog-receipt-sha256",
    ] {
        let mut args = cli_args();
        args.extend([option.into(), "value".into()]);
        assert!(parse(&args).is_err());
    }
    let raw = OsString::from_vec(b"/private/current-\xff.json".to_vec());
    let mut args = cli_args();
    args.extend([
        "--native-source-policy".into(),
        "retired-driver".into(),
        "--native-current-catalog-receipt".into(),
        raw.clone(),
        "--native-current-catalog-receipt-sha256".into(),
        "a".repeat(64).into(),
    ]);
    let selected = parse(&args).unwrap().unwrap();
    assert_eq!(selected.source_policy, SourcePolicy::RetiredDriver);
    assert_eq!(
        selected.current_catalog_receipt.unwrap(),
        (raw.into(), "a".repeat(64))
    );
    args[cli_args().len() + 1] = "retired-driving".into();
    assert!(parse(&args).is_err());
}

fn required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("cloud must set {name}"))
}

#[test]
fn original_four_case_reference_is_independent_and_sealed() {
    let root = PathBuf::from(required("DRIVER_DAEMON_ORIGINAL_CAPTURE"));
    let hash = required("DRIVER_DAEMON_ORIGINAL_MANIFEST_SHA256");
    let source = required("DRIVER_DAEMON_SOURCE_COMMIT");
    let original = OriginalCapture::open(&root, &hash, &source).unwrap();
    for (width, raw, frames) in [(1344, 1, 4), (1344, 0, 1), (1928, 1, 4), (1928, 0, 1)] {
        let prefix = format!("{width}-raw-{raw}");
        assert!(!original
            .read(&format!("{prefix}/reference/expected-0.bin"))
            .unwrap()
            .is_empty());
        for index in 0..frames {
            assert!(!original
                .read(&format!("{prefix}/frame-{index}.bin"))
                .unwrap()
                .is_empty());
            assert!(!original
                .read(&format!("{prefix}/reference/expected-{}.bin", index + 1))
                .unwrap()
                .is_empty());
        }
    }
    original.verify().unwrap();
}

#[test]
fn mutation_of_original_packet_and_manifest_is_rejected() {
    let source = PathBuf::from(required("DRIVER_DAEMON_ORIGINAL_CAPTURE"));
    let hash = required("DRIVER_DAEMON_ORIGINAL_MANIFEST_SHA256");
    let commit = required("DRIVER_DAEMON_SOURCE_COMMIT");
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let manifest = fs::read(source.join("reference-manifest.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    fs::write(root.join("reference-manifest.json"), &manifest).unwrap();
    for name in parsed["files"].as_object().unwrap().keys() {
        let target = root.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(name), target).unwrap();
    }
    let original = OriginalCapture::open(root, &hash, &commit).unwrap();
    let target = root.join("1344-raw-1/reference/expected-1.bin");
    let mut bytes = fs::read(&target).unwrap();
    bytes[0] ^= 1;
    fs::write(&target, bytes).unwrap();
    assert!(
        original.verify().is_err(),
        "mutated source-computed packet was accepted"
    );
    assert!(OriginalCapture::open(root, &hash, &commit).is_err());
    fs::copy(source.join("1344-raw-1/reference/expected-1.bin"), &target).unwrap();
    fs::write(root.join("reference-manifest.json"), b"{}").unwrap();
    assert!(OriginalCapture::open(root, &hash, &commit).is_err());
}

#[test]
fn actual_native_driver_replays_all_four_original_cases() {
    let options = Options {
        binary: required("DRIVER_DAEMON_BINARY").into(),
        catalog: required("DRIVER_DAEMON_CATALOG").into(),
        collector: Some(required("DRIVER_DAEMON_COLLECTOR").into()),
        peer: required("DRIVER_DAEMON_PEER").into(),
        models: required("DRIVER_DAEMON_MODELS").into(),
        original_dir: required("DRIVER_DAEMON_ORIGINAL_CAPTURE").into(),
        original_manifest_sha256: required("DRIVER_DAEMON_ORIGINAL_MANIFEST_SHA256"),
        source_commit: required("DRIVER_DAEMON_SOURCE_COMMIT"),
        source_policy: openpilot_dev_tools::driver_daemon_check::SourcePolicy::Present,
        current_catalog_receipt: None,
        output: required("DRIVER_DAEMON_NATIVE_OUTPUT").into(),
        root: required("DRIVER_DAEMON_ROOT").into(),
    };
    let report = run(&options).unwrap();
    assert!(
        !report["inputs"]["imported_python_sources"]
            .as_array()
            .unwrap()
            .is_empty(),
        "captured Python imports were not bound"
    );
    let runs = report["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 4);
    for (run, fields) in runs.iter().zip([188, 47, 188, 47]) {
        assert_eq!(run["compared_fields"], fields);
        assert_eq!(run["logging"]["source_calls"]["compared_records"], 5);
        assert_eq!(run["logging"]["disk_records"], 5);
    }
}
