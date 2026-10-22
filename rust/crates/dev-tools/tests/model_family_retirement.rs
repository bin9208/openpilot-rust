//! Hosted same-ELF six-source closure using fixed independent observations.
#![cfg(all(target_os = "linux", feature = "driver-daemon-native"))]
#[path = "../src/model_family_retirement_diagnostic.rs"]
mod diagnostic;
#[path = "../src/model_family_retirement_support.rs"]
mod support;
use openpilot_dev_tools::{driver_daemon_check, model_startup_check, usbgpu_daemon_check};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
};
use support::{
    absence, pair, pin, read_json, replace, required, run, success, success_usbgpu, text,
    write_json, Held, OwnedPath,
};

const COLLECTOR_SOURCE: &str = "2ecf3f6813f601ea52e7a61ccf75219952daefa0";
const COLLECTOR_MANIFEST: &str = "532281d6682b2d5a4ae8b8acf354b3a39d04eaf98cd49cc9a2c1432be30b8795";
const NO_COLLECTOR_SOURCE: &str = "481ae6fec39ecc68786c4c2813b1205fb4567141";
const NO_COLLECTOR_MANIFEST: &str =
    "1b8e6cea511e9f212cd7737688b3e2ec0a165b9ff985b492f0e98c4639f18380";

struct Inputs {
    root: PathBuf,
    preparation: PathBuf,
    original: PathBuf,
    binaries: PathBuf,
    driver_receipt: PathBuf,
    family_receipt: PathBuf,
    startup_manifest: String,
    usb_manifest: String,
}
impl Inputs {
    fn driver_args(&self, root: &Path, collector: bool, output: &Path) -> Vec<OsString> {
        let fixture = self.preparation.join("driver-fixture");
        let mut args = Vec::new();
        for (key, value) in [
            (
                "--binary",
                self.binaries.join("openpilot-dmonitoringmodeld"),
            ),
            ("--catalog", self.preparation.join("driver-catalog/catalog")),
            ("--models", fixture.join("no-collector/models")),
            ("--output", output.to_owned()),
            (
                "--native-peer",
                self.preparation.join("peer/driver-daemon-peer"),
            ),
            (
                "--native-original-dir",
                fixture.join(if collector {
                    "collector-current/observations"
                } else {
                    "no-collector/observations"
                }),
            ),
            (
                "--native-original-manifest-sha256",
                PathBuf::from(if collector {
                    COLLECTOR_MANIFEST
                } else {
                    NO_COLLECTOR_MANIFEST
                }),
            ),
            (
                "--native-source-commit",
                PathBuf::from(if collector {
                    COLLECTOR_SOURCE
                } else {
                    NO_COLLECTOR_SOURCE
                }),
            ),
            ("--native-root", root.to_owned()),
            ("--native-source-policy", PathBuf::from("retired-driver")),
            (
                "--native-current-catalog-receipt",
                self.driver_receipt.clone(),
            ),
            (
                "--native-current-catalog-receipt-sha256",
                PathBuf::from(pin(&self.driver_receipt)["sha256"].as_str().unwrap()),
            ),
        ] {
            pair(&mut args, key, value);
        }
        if collector {
            pair(
                &mut args,
                "--collector",
                self.binaries.join("openpilot-logmessaged"),
            );
        }
        args
    }
    fn family_args(&self, root: &Path, startup: bool, output: &Path) -> Vec<OsString> {
        let mut args = Vec::new();
        for (key, value) in [
            ("--catalog", self.preparation.join("family-catalog/catalog")),
            (
                "--native-original",
                self.original.join(if startup {
                    "startup/original"
                } else {
                    "usbgpu/original"
                }),
            ),
            (
                "--native-original-sha256",
                PathBuf::from(if startup {
                    &self.startup_manifest
                } else {
                    &self.usb_manifest
                }),
            ),
            ("--native-root", root.to_owned()),
            ("--source-policy", PathBuf::from("retired-model-family")),
        ] {
            pair(&mut args, key, value);
        }
        if startup {
            for (key, value) in [
                ("--driving", self.binaries.join("openpilot-driving-modeld")),
                (
                    "--driver",
                    self.binaries.join("openpilot-dmonitoringmodeld"),
                ),
                (
                    "--native-driving-peer",
                    self.preparation.join("peer/driving-daemon-peer"),
                ),
                (
                    "--native-driver-peer",
                    self.preparation.join("peer/driver-daemon-peer"),
                ),
                ("--output", output.to_owned()),
                ("--expect", PathBuf::from("pre-frame")),
            ] {
                pair(&mut args, key, value);
            }
        } else {
            for (key, value) in [
                (
                    "--binary",
                    self.binaries.join("examples/usbgpu_daemon_fixture"),
                ),
                (
                    "--worker",
                    self.binaries.join("examples/worker_protocol_fixture"),
                ),
                ("--peer", self.preparation.join("peer/driving-daemon-peer")),
                (
                    "--model",
                    self.preparation.join(format!(
                        "usb-bundle/usb-assets/models/{}/model.pkl",
                        usbgpu_daemon_check::MODEL_SHA
                    )),
                ),
                (
                    "--runtime",
                    self.preparation.join("usb-bundle/usb-runtime.tar.gz"),
                ),
                (
                    "--installed",
                    self.preparation.join("usb-bundle/usb-installed.json"),
                ),
                ("--assets", self.preparation.join("usb-bundle/usb-assets")),
                ("--evidence", output.to_owned()),
            ] {
                pair(&mut args, key, value);
            }
        }
        args
    }
    fn family_cli(&self, startup: bool) -> PathBuf {
        self.binaries.join(if startup {
            "check-model-startup"
        } else {
            "check-usbgpu-daemon"
        })
    }
}
fn admit_family_data(
    directory: &Path,
    expected: &str,
    kind: &str,
    source: &str,
    held: &mut Vec<Held>,
) -> Value {
    let path = directory.join("manifest.json");
    let manifest = Held::open(&path);
    assert_eq!(manifest.seal["sha256"], expected);
    held.push(manifest);
    let value = read_json(&path);
    assert_eq!(value["kind"], kind);
    assert_eq!(value["origin"], "unchanged original checker");
    assert_eq!(value["native_port_generated_expected"], false);
    assert_eq!(value["source_commit"], source);
    let files = value["files"].as_object().unwrap();
    assert!(!files.is_empty() && files.len() <= 256 && files.contains_key("report.json"));
    for (name, seal) in files {
        assert!(Path::new(name)
            .components()
            .all(|p| matches!(p, Component::Normal(_))));
        let file = Held::open(&directory.join(name));
        assert_eq!(file.seal, *seal, "independent data changed: {name}");
        held.push(file);
    }
    value
}
fn remember(
    label: &str,
    output: &Path,
    checked: &Value,
    accepted: &mut Vec<Value>,
    reports: &mut Vec<Held>,
) {
    let requested = output.join("report.json");
    // Raw output aliases are deliberately removed after their lane. Hold the
    // actual report under its canonical owned root, while preserving argv bytes.
    let report = requested.canonicalize().unwrap();
    let held = Held::open(&report);
    assert_eq!(
        read_json(&report),
        *checked,
        "accepted report changed after comparison"
    );
    held.verify();
    accepted.push(json!({"label":label,"report":report,"requested_report_bytes":requested.as_os_str().as_bytes(),"seal":held.seal}));
    reports.push(held);
}

#[test]
#[ignore = "hosted postdelete original observations, separate current catalogs and genuine USB assets required"]
fn same_elf_replays_all_three_native_clis_in_both_absent_roots() {
    let root = required("MODEL_FAMILY_GATE_ROOT");
    let preparation = required("MODEL_FAMILY_GATE_PREPARATION");
    let inputs = Inputs {
        binaries: root.join("rust/target/debug"),
        root,
        original: required("MODEL_FAMILY_GATE_ORIGINAL"),
        driver_receipt: preparation.join("driver-catalog/current-catalog-receipt.json"),
        family_receipt: preparation.join("family-catalog/current-catalog-receipt.json"),
        preparation,
        startup_manifest: text("MODEL_FAMILY_STARTUP_MANIFEST_SHA256"),
        usb_manifest: text("MODEL_FAMILY_USBGPU_MANIFEST_SHA256"),
    };
    assert!(
        inputs.root.is_absolute()
            && inputs.preparation.is_absolute()
            && inputs.original.is_absolute()
    );
    let proof = inputs.preparation.join("grouped-native");
    fs::create_dir(&proof).unwrap();
    let mut union = BTreeMap::new();
    for (name, hash) in driver_daemon_check::RETIRED
        .into_iter()
        .chain(model_startup_check::RETIRED)
        .chain(usbgpu_daemon_check::RETIRED)
    {
        if let Some(old) = union.insert(name, hash) {
            assert_eq!(old, hash);
        }
    }
    assert_eq!(union.len(), 6);
    let retired = union
        .keys()
        .map(|s| s.strip_prefix("rust/tools/").unwrap())
        .collect::<Vec<_>>();
    absence(&inputs.root, &retired);
    let mut held = Vec::new();
    for name in [
        "check-driver-daemon",
        "check-model-startup",
        "check-usbgpu-daemon",
        "openpilot-driving-modeld",
        "openpilot-dmonitoringmodeld",
        "openpilot-logmessaged",
        "model-run",
        "examples/usbgpu_daemon_fixture",
        "examples/worker_protocol_fixture",
    ] {
        held.push(Held::open(&inputs.binaries.join(name)));
    }
    for path in [
        inputs.preparation.join("peer/driving-daemon-peer"),
        inputs.preparation.join("peer/driver-daemon-peer"),
        inputs.driver_receipt.clone(),
        inputs.family_receipt.clone(),
    ] {
        held.push(Held::open(&path));
    }
    let startup_manifest = admit_family_data(
        &inputs.original.join("startup/original"),
        &inputs.startup_manifest,
        "startup",
        &text("MODEL_FAMILY_CAPTURE_SOURCE"),
        &mut held,
    );
    let usb_manifest = admit_family_data(
        &inputs.original.join("usbgpu/original"),
        &inputs.usb_manifest,
        "usbgpu",
        &text("MODEL_FAMILY_CAPTURE_SOURCE"),
        &mut held,
    );
    assert_eq!(startup_manifest["models"], usb_manifest["models"]);
    let family_receipt = read_json(&inputs.family_receipt);
    assert_eq!(family_receipt["models"], startup_manifest["models"]);
    assert_eq!(family_receipt["models"].as_object().unwrap().len(), 5);
    assert_eq!(
        read_json(&inputs.driver_receipt)["models"]
            .as_object()
            .unwrap()
            .len(),
        4
    );
    for (receipt, models) in [
        (
            &inputs.family_receipt,
            inputs.preparation.join("driving-fixture/models"),
        ),
        (
            &inputs.driver_receipt,
            inputs
                .preparation
                .join("driver-fixture/no-collector/models"),
        ),
    ] {
        for (name, seal) in read_json(receipt)["models"].as_object().unwrap() {
            assert!(Path::new(name)
                .components()
                .all(|p| matches!(p, Component::Normal(_))));
            let model = Held::open(&models.join(name));
            assert_eq!(model.seal, *seal);
            held.push(model);
        }
    }
    // No relabelling or model substitution: both driver datasets stay independently bound.
    let mut driver_originals = Vec::new();
    for (dataset, source, sha) in [
        ("collector-current", COLLECTOR_SOURCE, COLLECTOR_MANIFEST),
        ("no-collector", NO_COLLECTOR_SOURCE, NO_COLLECTOR_MANIFEST),
    ] {
        let original = driver_daemon_check::OriginalCapture::open(
            &inputs
                .preparation
                .join("driver-fixture")
                .join(dataset)
                .join("observations"),
            sha,
            source,
        )
        .unwrap();
        original.verify().unwrap();
        driver_originals.push(original);
    }
    let source_startup = read_json(&inputs.original.join("startup/original/report.json"));
    let source_usb = read_json(&inputs.original.join("usbgpu/original/report.json"));
    assert_eq!(source_startup["runs"].as_array().unwrap().len(), 8);
    assert_eq!(source_usb["runs"].as_array().unwrap().len(), 6);
    let private = inputs.preparation.join("private-root");
    let copied = support::copy_source(&inputs.root, &private, &proof);
    for name in [
        "check-driver-daemon",
        "check-model-startup",
        "check-usbgpu-daemon",
    ] {
        for (label, args, status) in [
            ("help", vec![OsString::from("--help")], 0),
            ("missing", vec![], 2),
            ("unknown", vec![OsString::from("--unknown")], 2),
        ] {
            let result = run(
                &inputs.binaries.join(name),
                &args,
                &proof.join(format!("admission-{name}-{label}")),
                Some(&inputs.family_receipt),
                true,
            );
            assert_eq!(
                result.code,
                status,
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            if status == 0 {
                assert!(String::from_utf8_lossy(&result.stdout).contains("Usage:"));
            }
        }
    }
    let mut accepted = Vec::new();
    let mut accepted_reports = Vec::new();
    let mut negatives = 0;
    for (number, source) in [&inputs.root, &private].into_iter().enumerate() {
        absence(source, &retired);
        let lane = proof.join(format!("root-{number}"));
        fs::create_dir(&lane).unwrap();
        let (raw, raw_owner) = support::raw_alias(&lane);
        for collector in [true, false] {
            let mode = if collector {
                "collector"
            } else {
                "no-collector"
            };
            for (alias, directory) in [("ordinary", &lane), ("raw", &raw)] {
                let label = format!("root-{number}-driver-{mode}-{alias}");
                let output = directory.join(format!("driver-{mode}-{alias}-output"));
                let result = run(
                    &inputs.binaries.join("check-driver-daemon"),
                    &inputs.driver_args(source, collector, &output),
                    &proof.join(&label),
                    Some(&inputs.driver_receipt),
                    false,
                );
                let report = success(&result, &output, 4);
                assert_eq!(report["source_policy"], "retired-driver");
                assert_eq!(
                    report["source_commit"],
                    if collector {
                        COLLECTOR_SOURCE
                    } else {
                        NO_COLLECTOR_SOURCE
                    }
                );
                assert_eq!(
                    report["current_catalog"]["receipt"]["sha256"],
                    pin(&inputs.driver_receipt)["sha256"]
                );
                for row in report["runs"].as_array().unwrap() {
                    assert_eq!(row["logging"].is_null(), !collector);
                    if collector {
                        assert_ne!(report["checkout_commit"], report["source_commit"]);
                        assert_eq!(row["logging"]["source_commit"], report["checkout_commit"]);
                    }
                }
                remember(
                    &label,
                    &output,
                    &report,
                    &mut accepted,
                    &mut accepted_reports,
                );
            }
        }
        for startup in [true, false] {
            let kind = if startup { "startup" } else { "usbgpu" };
            let cli = inputs.family_cli(startup);
            let output = lane.join(format!("{kind}-output"));
            let label = format!("root-{number}-{kind}");
            let args = inputs.family_args(source, startup, &output);
            let result = run(
                &cli,
                &args,
                &proof.join(&label),
                Some(&inputs.family_receipt),
                false,
            );
            let report = if startup {
                success(&result, &output, 8)
            } else {
                success_usbgpu(&result, &output)
            };
            assert_eq!(report["source_absence"], true);
            assert_eq!(
                report["original_manifest_sha256"],
                if startup {
                    &inputs.startup_manifest
                } else {
                    &inputs.usb_manifest
                }
                .as_str()
            );
            if startup {
                for row in report["runs"].as_array().unwrap() {
                    assert_eq!(row["native_loaded_before_car_params"], true);
                    assert_eq!(row["native_loaded_without_frame"], true);
                    assert_eq!(row["frames_sent_to_native"], 0);
                    assert_eq!(row["native_exit"], 0);
                    let matches = source_startup["runs"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|r| {
                            r["component"] == row["component"]
                                && r["resolution"] == row["resolution"]
                                && r["raw"] == row["raw"]
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(matches.len(), 1);
                    assert_eq!(row["source"], matches[0]["source"]);
                    assert_eq!(row["source_first_frame"], matches[0]["source_first_frame"]);
                }
            } else {
                for row in report["runs"].as_array().unwrap() {
                    assert_eq!(row["passed"], true);
                    assert_eq!(row["workers_reaped_and_shared_removed"], true);
                    let matches = source_usb["runs"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|r| r["camera"] == row["camera"] && r["mode"] == row["mode"])
                        .collect::<Vec<_>>();
                    assert_eq!(matches.len(), 1);
                    assert_eq!(row["final"], matches[0]["final"]);
                    if row["mode"] != "cancel" {
                        assert_eq!(row["initial"], matches[0]["initial"]);
                        assert_eq!(row["packets"], matches[0]["packets"]);
                    } else {
                        assert_eq!(row["packets"], json!([]));
                    }
                }
            }
            remember(
                &label,
                &output,
                &report,
                &mut accepted,
                &mut accepted_reports,
            );
            let bad_output = lane.join(format!("{kind}-bad-policy-output"));
            let mut bad = inputs.family_args(source, startup, &bad_output);
            replace(&mut bad, "--source-policy", "unrecognized-policy");
            let result = run(
                &cli,
                &bad,
                &proof.join(format!("root-{number}-{kind}-bad-policy")),
                Some(&inputs.family_receipt),
                true,
            );
            assert_eq!(result.code, 1);
            assert!(result.stdout.is_empty());
            assert!(!bad_output.exists());
            assert!(String::from_utf8_lossy(&result.stderr).contains("source policy"));
            let foreign = lane.join(format!("{kind}-foreign-output"));
            fs::create_dir(&foreign).unwrap();
            fs::write(foreign.join("report.json"), b"foreign-owned-report").unwrap();
            let result = run(
                &cli,
                &inputs.family_args(source, startup, &foreign),
                &proof.join(format!("root-{number}-{kind}-foreign-output")),
                Some(&inputs.family_receipt),
                true,
            );
            assert_eq!(result.code, 1);
            assert!(result.stdout.is_empty());
            assert!(String::from_utf8_lossy(&result.stderr).contains("exists"));
            assert_eq!(
                fs::read(foreign.join("report.json")).unwrap(),
                b"foreign-owned-report"
            );
        }
        for (index, name) in retired.iter().enumerate() {
            let path = source.join("rust/tools").join(name);
            for link in [false, true] {
                let target = lane.join("absent-target");
                let owner = OwnedPath::reappearance(&path, link.then_some(target.as_path()));
                let output = lane.join(format!("negative-{index}-{link}-output"));
                let startup = matches!(
                    *name,
                    "check_model_startup.py" | "model_startup_reference.py"
                );
                let family = startup || *name == "check_usbgpu_daemon.py";
                let (cli, args, receipt) = if family {
                    (
                        inputs.family_cli(startup),
                        inputs.family_args(source, startup, &output),
                        &inputs.family_receipt,
                    )
                } else {
                    (
                        inputs.binaries.join("check-driver-daemon"),
                        inputs.driver_args(source, false, &output),
                        &inputs.driver_receipt,
                    )
                };
                let result = run(
                    &cli,
                    &args,
                    &proof.join(format!("root-{number}-negative-{index}-{link}")),
                    Some(receipt),
                    true,
                );
                assert_eq!(
                    result.code,
                    1,
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                assert!(result.stdout.is_empty());
                assert!(!output.exists());
                let expected = if family {
                    "retired model family source reappeared"
                } else {
                    "retired driver source present"
                };
                assert!(String::from_utf8_lossy(&result.stderr).contains(expected));
                drop(owner);
                assert!(fs::symlink_metadata(&path)
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound));
                negatives += 1;
            }
        }
        drop(raw_owner);
        assert!(fs::symlink_metadata(&raw).is_err());
        absence(source, &retired);
        support::verify_source(source, &copied);
        for input in &held {
            input.verify();
        }
        for original in &driver_originals {
            original.verify().unwrap();
        }
    }
    assert_eq!(negatives, 2 * retired.len() * 2);
    for root in [&inputs.root, &private] {
        absence(root, &retired);
        support::verify_source(root, &copied);
    }
    assert_eq!(accepted_reports.len(), accepted.len());
    for report in &accepted_reports {
        report.verify();
    }
    write_json(
        &proof.join("summary.json"),
        &json!({"passed":true,"same_elfs":held.iter().map(Held::evidence).collect::<Vec<_>>(),"accepted":accepted,"roots":2,"driver_case_runs_per_root":16,"startup_case_runs_per_root":8,"usb_case_runs_per_root":6,"reappearance_controls":negatives,"execution_environment":{"path":"/nonexistent","removed_variables":["PYTHONPATH","PYTHONHOME"],"interpreter_execution":"not_measured","exec_trace_collected":false},"source_absence":{"roots_verified":2,"retired_paths_per_root":retired.len(),"after_traffic":true},"accepted_reports_final_verified":accepted_reports.len(),"different_model_datasets":true,"scope":"USB unchanged-controller assertions against native protocol fixture; startup independent source initialization; no device proof"}),
    );
}

#[test]
fn committed_original_data_preserves_all_captured_byte_seals() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model-family-original");
    let mut held = Vec::new();
    for (kind, expected) in [
        (
            "startup",
            "4c54c0d6fb375cec2d64c6ecfeb5ce709f223bbe0aeb0114b68c5a4e184c7416",
        ),
        (
            "usbgpu",
            "8774d6752cabaeba96bacd4bab009a9432f12e5231e8da608285ff5115b21ad8",
        ),
    ] {
        admit_family_data(
            &root.join(kind).join("original"),
            expected,
            kind,
            "0ec6cbae2d85fd98f2e0894697de7869f6159f51",
            &mut held,
        );
    }
    assert_eq!(held.len(), 116); // Two manifests plus114 independently sealed data files.
    for file in held {
        file.verify();
    }
}

#[test]
#[ignore = "failure-only same-host driver diagnostic; never retirement acceptance"]
fn diagnose_first_driver_frame_on_same_host_and_catalog() {
    diagnostic::run_first_driver_frame_diagnostic()
        .expect("failure-only driver diagnostic did not complete; inspect retained receipts");
}
