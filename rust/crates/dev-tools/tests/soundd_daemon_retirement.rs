//! Cloud-only comparison against an independently captured original report.
#![cfg(feature = "soundd-daemon-native")]
use openpilot_dev_tools::soundd_daemon_check::{self as check, Event, Mode};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const RETIRED: &str = "rust/tools/check_soundd_daemon.py";
const ORIGINAL_SHA: &str = "fadc1ad42b382d4e4a6be128ead810e2b694e69b937525cb280e8f59cd799664";
const REPORT_SHA: &str = "53c8329d9aa7684deeefe987e5dd5662b307bb67cfdb50b8f3f55a6931e63a0f";
const PROVENANCE_SHA: &str = "e40fb117d34b6c64cac54443e042c63fdc962fab05a85eef9f5ab174dca7aa1a";

fn required(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("missing required {name}")))
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn events(path: &Path) -> Vec<Event> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn stable(report: &Value) -> Value {
    let rows = report.as_array().unwrap();
    assert_eq!(rows.len(), Mode::ALL.len());
    Value::Array(rows.iter().zip(Mode::ALL).map(|(row, mode)| {
        assert_eq!(row.as_object().unwrap().len(), 9);
        assert_eq!(row["mode"], mode.as_str());
        assert_eq!(row["passed"], true);
        assert_eq!(row["audio_hardware_opened"], false);
        assert!(row["pid"].as_u64().unwrap() > 0);
        assert!(row["callbacks"].as_u64().is_some());
        let packets = row["packets"].as_array().unwrap();
        assert_eq!(packets.len(), if mode == Mode::Live { 10 } else { 0 });
        let packets = packets.iter().map(|packet| {
            assert_eq!(packet.as_object().unwrap().len(), 3);
            assert!(packet["time"].as_f64().unwrap().is_finite());
            json!({"topic": packet["topic"], "values": packet["values"]})
        }).collect::<Vec<_>>();
        json!({"mode":row["mode"],"passed":row["passed"],"exit":row["exit"],"opens":row["opens"],"packets":packets,"audio_hardware_opened":row["audio_hardware_opened"]})
    }).collect())
}
fn inspect_lane(output: &Path) -> Value {
    assert_eq!(
        fs::read(output.join("compile.log")).unwrap(),
        b"reused prebuilt owned PortAudio ABI fixture\n"
    );
    let report = read(&output.join("summary.json"));
    for (row, mode) in report.as_array().unwrap().iter().zip(Mode::ALL) {
        let case = output.join(mode.as_str());
        assert_eq!(read(&case.join("summary.json")), *row);
        let metrics = check::validate_events(
            mode,
            row["pid"].as_u64().unwrap().try_into().unwrap(),
            row["exit"].as_i64().unwrap().try_into().unwrap(),
            &events(&case.join("events.jsonl")),
        )
        .unwrap();
        assert_eq!(row["opens"], metrics.opens);
        assert_eq!(row["callbacks"], metrics.callbacks);
        assert_eq!(row["median_period"], json!(metrics.median_period));
        assert!(case.join("stdout.log").is_file());
        assert!(case.join("stderr.log").is_file());
    }
    stable(&report)
}

fn audit_native_exec(output: &Path, trace_path: &Path) {
    let checker = required("SOUNDD_DAEMON_WORKER_BINARY");
    let daemon = required("SOUNDD_DAEMON_BINARY");
    let assets = required("SOUNDD_FIXTURE_ASSETS_BINARY");
    let library = required("SOUNDD_PORTAUDIO_LIBRARY");
    let names = [checker, daemon, assets, library].map(|path| {
        path.to_str()
            .expect("native executable path is Unicode")
            .to_owned()
    });
    let [checker, daemon, assets, library] = names;
    let trace = fs::read_to_string(trace_path).unwrap();
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let invocation = regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), "#).unwrap();
    assert_eq!(
        opening.find_iter(&trace).count(),
        22,
        "wrong exec attempt count"
    );
    let mut seen = 0;
    let mut checker_cli = 0;
    let mut worker_modes = BTreeMap::new();
    let mut helper_roots = BTreeMap::new();
    let mut daemon_roots = BTreeMap::new();
    for capture in invocation.captures_iter(&trace) {
        seen += 1;
        let program: String = serde_json::from_str(&capture[1]).unwrap();
        let argv: Vec<String> = serde_json::from_str(&capture[2]).unwrap();
        assert_eq!(
            argv.first(),
            Some(&program),
            "exec argv0 differs from executable"
        );
        if program == checker && argv.get(1).is_some_and(|s| s == "--binary") {
            checker_cli += 1;
            assert_eq!(
                argv,
                vec![
                    checker.clone(),
                    "--binary".into(),
                    daemon.clone(),
                    "--output".into(),
                    output.to_str().unwrap().into(),
                    "--fixture-assets-binary".into(),
                    assets.clone(),
                    "--portaudio-library".into(),
                    library.clone()
                ]
            );
        } else if program == checker && argv.get(1).is_some_and(|s| s == "--soundd-daemon-worker") {
            assert_eq!(argv.len(), 9);
            let mode = Mode::parse(&argv[2]).unwrap();
            assert_eq!(argv[3], daemon);
            assert_eq!(argv[4], library);
            assert_eq!(Path::new(&argv[5]), output.join(mode.as_str()));
            assert_eq!(argv[6], assets);
            assert!(argv[7].starts_with("sound-"));
            let temporary = Path::new(&argv[8]);
            assert!(temporary.is_absolute());
            assert!(temporary
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("sound-native-"));
            assert!(
                worker_modes.insert(argv[8].clone(), mode).is_none(),
                "duplicate worker temporary root"
            );
        } else if program == assets {
            assert_eq!(argv.len(), 4);
            assert_eq!(argv[2], "48000");
            assert_eq!(argv[3], "1");
            *helper_roots.entry(argv[1].clone()).or_insert(0usize) += 1;
        } else if program == daemon {
            assert!(argv.len() == 5 || argv.len() == 7);
            assert_eq!(argv[1], "--assets");
            assert_eq!(argv[3], "--portaudio-library");
            assert_eq!(argv[4], library);
            let base = Path::new(&argv[2]);
            let root = base.parent().unwrap().parent().unwrap().parent().unwrap();
            assert_eq!(base, root.join("openpilot/selfdrive/assets"));
            let key = root.to_str().unwrap().to_owned();
            let mode = worker_modes
                .get(&key)
                .expect("daemon ran without observed worker root");
            if matches!(mode, Mode::Bounded | Mode::Retry) {
                assert_eq!(argv.len(), 7);
                assert_eq!(argv[5], "--cycles");
                assert_eq!(argv[6], "5");
            } else {
                assert_eq!(argv.len(), 5);
            }
            *daemon_roots.entry(key).or_insert(0usize) += 1;
        } else {
            panic!("unexpected executable: {program}");
        }
    }
    assert_eq!(seen, 22, "opaque or execveat execution attempt");
    assert_eq!(checker_cli, 1);
    assert_eq!(worker_modes.len(), Mode::ALL.len());
    for mode in Mode::ALL {
        assert_eq!(worker_modes.values().filter(|&&m| m == mode).count(), 1);
    }
    for root in worker_modes.keys() {
        assert_eq!(helper_roots.get(root), Some(&1));
        assert_eq!(daemon_roots.get(root), Some(&1));
    }
    assert_eq!(helper_roots.len(), 7);
    assert_eq!(daemon_roots.len(), 7);
}

#[test]
fn actual_original_present_and_absent_reports_match_sealed_observation() {
    let captured = include_bytes!("fixtures/soundd-daemon/original-seven-f60.json");
    assert_eq!(format!("{:x}", Sha256::digest(captured)), REPORT_SHA);
    let provenance_bytes =
        include_bytes!("fixtures/soundd-daemon/original-seven-f60.provenance.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(provenance_bytes)),
        PROVENANCE_SHA
    );
    let provenance: Value = serde_json::from_slice(provenance_bytes).unwrap();
    assert_eq!(provenance["candidate_executed_for_expectations"], false);
    assert_eq!(provenance["original_report_sha256"], REPORT_SHA);
    assert_eq!(provenance["original_checker_sha256"], ORIGINAL_SHA);
    let captured: Value = serde_json::from_slice(captured).unwrap();
    let expected = stable(&captured);
    let original = required("SOUNDD_ORIGINAL_OUTPUT");
    let present = required("SOUNDD_NATIVE_PRESENT_OUTPUT");
    let absent = required("SOUNDD_NATIVE_ABSENT_OUTPUT");
    for output in [&original, &present, &absent] {
        assert_eq!(inspect_lane(output), expected, "{}", output.display());
    }
    let present_root = required("SOUNDD_NATIVE_PRESENT_ROOT");
    let absent_root = required("SOUNDD_NATIVE_ABSENT_ROOT");
    let present_source = check::validate_source(&present_root).unwrap();
    let absent_source = check::validate_source(&absent_root).unwrap();
    assert_eq!(present_source.as_object().unwrap().len(), 21);
    assert_eq!(present_source[RETIRED], ORIGINAL_SHA);
    assert_eq!(absent_source.as_object().unwrap().len(), 20);
    assert!(absent_source.get(RETIRED).is_none());
    let mut expected_absent = present_source.clone();
    expected_absent.as_object_mut().unwrap().remove(RETIRED);
    assert_eq!(absent_source, expected_absent);
    assert!(!absent_root.join(RETIRED).exists());
    let present_provenance = read(&present.join("provenance.json"));
    let absent_provenance = read(&absent.join("provenance.json"));
    assert_eq!(present_provenance["source_sha256"], present_source);
    assert_eq!(absent_provenance["source_sha256"], absent_source);
    assert_eq!(
        present_provenance["historical_source_sha256"][RETIRED],
        ORIGINAL_SHA
    );
    assert_eq!(
        absent_provenance["historical_source_sha256"][RETIRED],
        ORIGINAL_SHA
    );
    assert_eq!(
        present_provenance["executables"],
        absent_provenance["executables"]
    );
    for (name, key) in [
        ("SOUNDD_DAEMON_BINARY", "daemon"),
        ("SOUNDD_DAEMON_WORKER_BINARY", "checker_worker"),
        ("SOUNDD_FIXTURE_ASSETS_BINARY", "fixture_assets"),
        ("SOUNDD_PORTAUDIO_LIBRARY", "owned_portaudio"),
    ] {
        let hash = format!("{:x}", Sha256::digest(fs::read(required(name)).unwrap()));
        assert_eq!(present_provenance["executables"][key], hash, "{name}");
    }
    assert_eq!(present_provenance["limits"], absent_provenance["limits"]);
    assert_eq!(present_provenance["physical_audio_tested"], false);
    assert_eq!(absent_provenance["physical_audio_tested"], false);
    audit_native_exec(&present, &required("SOUNDD_NATIVE_PRESENT_EXEC"));
    audit_native_exec(&absent, &required("SOUNDD_NATIVE_ABSENT_EXEC"));
}
